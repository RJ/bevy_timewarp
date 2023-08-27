use crate::prelude::*;
use bevy::prelude::*;
use std::time::Duration;

/// potentially-concurrent systems request rollbacks by writing a request
/// to the Events<RollbackRequest>, which we drain and use the smallest
/// frame that was requested - ie, covering all requested frames.
pub(crate) fn consolidate_rollback_requests(
    mut rb_events: ResMut<Events<RollbackRequest>>,
    mut commands: Commands,
    game_clock: Res<GameClock>,
) {
    let mut rb_frame: FrameNumber = 0;
    // NB: a manually managed event queue, which we drain here
    for ev in rb_events.drain() {
        if rb_frame == 0 || ev.0 < rb_frame {
            rb_frame = ev.0;
        }
    }
    if rb_frame == 0 {
        return;
    }
    commands.insert_resource(Rollback::new(rb_frame, game_clock.frame()));
}

/// wipes RemovedComponents<T> queue for component T.
/// useful during rollback, because we don't react to removals that are part of resimulating.
pub(crate) fn clear_removed_components_queue<T: Component>(
    mut e: RemovedComponents<T>,
    game_clock: Res<GameClock>,
) {
    if !e.is_empty() {
        debug!(
            "Clearing f:{:?} RemovedComponents<{}> during rollback: {:?}",
            game_clock.frame(),
            std::any::type_name::<T>(),
            e.len()
        );
    }
    e.clear();
}

/// add the ComponentHistory<T> and ServerSnapshot<T> whenever an entity gets the T component.
/// NB: you must have called `app.register_rollback::<T>()` for this to work.
pub(crate) fn add_timewarp_buffer_components<
    T: Component + Clone + std::fmt::Debug,
    const CORRECTION_LOGGING: bool,
>(
    q: Query<
        (Entity, &T),
        (
            Added<T>,
            Without<NotRollbackable>,
            Without<ComponentHistory<T>>,
        ),
    >,
    mut commands: Commands,
    game_clock: Res<GameClock>,
    timewarp_config: Res<TimewarpConfig>,
) {
    for (e, comp) in q.iter() {
        // insert component value at this frame, since the system that records it won't run
        // if a rollback is happening this frame. and if it does it just overwrites
        let mut comp_history = ComponentHistory::<T>::with_capacity(
            timewarp_config.rollback_window as usize,
            game_clock.frame(),
        );
        if CORRECTION_LOGGING {
            comp_history.enable_correction_logging();
        }
        comp_history.insert(game_clock.frame(), comp.clone(), &e);

        debug!(
            "Adding ComponentHistory<> to {e:?} for {:?}\nInitial val @ {:?} = {:?}",
            std::any::type_name::<T>(),
            game_clock.frame(),
            comp.clone(),
        );
        commands.entity(e).insert((
            comp_history,
            // server snapshots are sent event n frames, so there are going to be lots of Nones in
            // the sequence buffer. increase capacity accordingly.
            // TODO compute based on snapshot send rate.
            ServerSnapshot::<T>::with_capacity(timewarp_config.rollback_window as usize * 60), // TODO yuk
        ));
    }
}

/// record component lifetimes
/// won't be called first time comp is added, since it won't have a ComponentHistory yet.
/// only for comp removed ... then readded birth
pub(crate) fn record_component_added_to_alive_ranges<T: Component + Clone + std::fmt::Debug>(
    mut q: Query<(Entity, &mut ComponentHistory<T>), (Added<T>, Without<NotRollbackable>)>,
    game_clock: Res<GameClock>,
    rb: Option<Res<Rollback>>,
) {
    // during rollback, components are removed and readded.
    // but we don't want to log the same as outside of rollback, we want to ignore.
    // however this system still runs, so that the Added<T> filters update their markers
    // otherwise things added during rollback would all show as Added the first frame back.
    if rb.is_some() {
        return;
    }

    for (entity, mut ct) in q.iter_mut() {
        debug!(
            "{entity:?} Component birth @ {:?} {:?}",
            game_clock.frame(),
            std::any::type_name::<T>()
        );
        ct.report_birth_at_frame(game_clock.frame());
    }
}

/// Write current value of component to the ComponentHistory buffer for this frame
pub(crate) fn record_component_history_values<T: Component + Clone + std::fmt::Debug>(
    mut q: Query<(
        Entity,
        &T,
        &mut ComponentHistory<T>,
        Option<&mut TimewarpCorrection<T>>,
    )>,
    game_clock: Res<GameClock>,
    mut commands: Commands,
) {
    for (entity, comp, mut comp_hist, opt_correction) in q.iter_mut() {
        if comp_hist.correction_logging_enabled {
            if let Some(diff_frame) = comp_hist.diff_at_frame {
                if diff_frame == game_clock.frame() {
                    comp_hist.diff_at_frame = None;
                    // we must have rolled back and resimulated forward to a frame that already has
                    // data. before we replace what's in the comp history buffer for this frame, we
                    // take a look at it, and compute the diff. this represents the error in our simulation.
                    // since the change will cause the simulation to snap to the new values.
                    let old_val = comp_hist.at_frame(diff_frame).unwrap();
                    // need T to be PartialEq here:
                    // if *old_val != *comp {
                    if let Some(mut correction) = opt_correction {
                        correction.before = old_val.clone();
                        correction.after = comp.clone();
                        correction.frame = diff_frame; // current frame
                    } else {
                        commands.entity(entity).insert(TimewarpCorrection::<T> {
                            before: old_val.clone(),
                            after: comp.clone(),
                            frame: diff_frame,
                        });
                    }
                    // }
                }
            }
        }
        trace!("record @ {:?} {entity:?} {comp:?}", game_clock.frame());
        comp_hist.insert(game_clock.frame(), comp.clone(), &entity);
    }
}

/// when you need to insert a component at a previous frame, you wrap it up like:
/// InsertComponentAtFrame::<Shield>::new(frame, shield_component);
/// and this system handles things.
pub(crate) fn insert_components_at_prior_frames<T: Component + Clone + std::fmt::Debug>(
    mut q: Query<
        (
            Entity,
            &InsertComponentAtFrame<T>,
            Option<&mut ComponentHistory<T>>,
            Option<&mut ServerSnapshot<T>>,
        ),
        Added<InsertComponentAtFrame<T>>,
    >,
    mut commands: Commands,
    timewarp_config: Res<TimewarpConfig>,
    mut rb_ev: ResMut<Events<RollbackRequest>>,
) {
    for (entity, icaf, opt_ch, opt_ss) in q.iter_mut() {
        // warn!("{icaf:?}");
        let mut ent_cmd = commands.entity(entity);
        ent_cmd.remove::<InsertComponentAtFrame<T>>();

        // if the entity never had this component type T before, we'll need to insert
        // the ComponentHistory and ServerSnapshot components.
        // If they already exist, just insert at the correct frame.
        if let Some(mut ch) = opt_ch {
            ch.insert_authoritative(icaf.frame, icaf.component.clone(), &entity);
            ch.report_birth_at_frame(icaf.frame);
            trace!("Inserting component at past frame for existing ComponentHistory");
        } else {
            let mut ch = ComponentHistory::<T>::with_capacity(
                timewarp_config.rollback_window as usize,
                icaf.frame,
            );
            ch.insert_authoritative(icaf.frame, icaf.component.clone(), &entity);
            ent_cmd.insert(ch);
            trace!("Inserting component at past frame by inserting new ComponentHistory");
        }

        if let Some(mut ss) = opt_ss {
            ss.insert(icaf.frame, icaf.component.clone());
        } else {
            let mut ss =
                ServerSnapshot::<T>::with_capacity(timewarp_config.rollback_window as usize * 60); // TODO yuk
            ss.insert(icaf.frame, icaf.component.clone());
            ent_cmd.insert(ss);
        }

        // trigger a rollback
        rb_ev.send(RollbackRequest(icaf.frame));
    }
}

/// - Has the ServerSnapshot changed?
/// - Does it contain a snapshot newer than the last authoritative frame in the component history?
/// - Does the snapshot value at that frame differ from the predicted values we used?
/// - If so, copy the snapshot value to ComponentHistory and trigger a rollback to that frame.
/// this system only concerns itself with non-Anachronous entities, meaning if we got a new
/// serversnapshot, we can do a rollback. no funny business with holding entities in the past.
pub(crate) fn apply_snapshots_and_rollback_for_non_anachronous<
    T: Component + Clone + std::fmt::Debug,
>(
    mut q: Query<
        (Entity, &mut ComponentHistory<T>, &ServerSnapshot<T>),
        (Changed<ServerSnapshot<T>>, Without<Anachronous>),
    >,
    game_clock: Res<GameClock>,
    mut rb_ev: ResMut<Events<RollbackRequest>>,
) {
    for (entity, mut comp_history, comp_server) in q.iter_mut() {
        // if the server snapshot component has been updated, and contains a newer authoritative
        // value than what we've already applied, we might need to rollback and resim.
        if comp_server.values.newest_frame() == 0 {
            // no data yet
            continue;
        }
        let new_snapshot_frame = comp_server.values.newest_frame();
        if comp_history.most_recent_authoritative_frame < new_snapshot_frame {
            let new_comp_val = comp_server.values.get(new_snapshot_frame).unwrap().clone();
            // copy from server snapshot to component history. in prep for rollback
            // TODO check if local predicted value matches snapshot and bypass!!
            comp_history.insert_authoritative(new_snapshot_frame, new_comp_val, &entity);
            // calculate error offset when we resimulate back to this frame.
            // ie, diff between current value of T at current frame, vs current frame post-rollback+resim.
            if comp_history.correction_logging_enabled {
                comp_history.diff_at_frame = Some(game_clock.frame());
            }

            // trigger a rollback
            rb_ev.send(RollbackRequest(new_snapshot_frame));
        }
    }
}

/// in the case of anachronous entities, it's no use filtering on Changed<ServerSnapshot> in order
/// to apply authoritative values to ComponentHistory
///
/// For example:
///
/// Say client game_clock.frame() is 100.
/// We get a packet of ServerSnapshot data for frame 96
/// Anachronous entities are running (eg) 6 frames lagged, so can't use that snapshot until
/// frame 96 + 6 = 102.
///
/// Also of note - ServerSnapshots are infrequent.
/// ie, the ServerSnapshot.values buffer is full of Nones for many frames.
///
/// So how do we apply snapshot data to anachronous entities correctly?
/// Do we put the frame+frames_ahead number into a resource when we detect a Changed<ServerSnapshot>?
///
/// Not sure - for now, we'll just check every frame by calculating our target frame and checking
/// if a snapshot exists, and snapping the present state to the serversnapshot value.
///
pub(crate) fn apply_snapshots_and_snap_for_anachronous<T: Component + Clone + std::fmt::Debug>(
    mut q: Query<(
        Entity,
        &mut T,
        &mut ComponentHistory<T>,
        &ServerSnapshot<T>,
        &Anachronous,
    )>,
    game_clock: Res<GameClock>,
) {
    for (entity, mut comp, mut comp_history, comp_server, anach) in q.iter_mut() {
        if comp_server.values.newest_frame() == 0 {
            // no data yet
            continue;
        }
        // we are running this entity delayed, this is the frame number that we treat as current:
        let target_frame = game_clock.frame().saturating_sub(anach.frames_behind);

        if comp_history.most_recent_authoritative_frame >= target_frame {
            continue;
        }

        // is there a snapshot value for our target_frame?
        if let Some(new_comp_val) = comp_server.values.get(target_frame) {
            trace!(
                "f={:?} SNAPPING ANACHRONOUS for {target_frame}",
                game_clock.frame()
            );
            // we are taking this new_comp_val, which originates from target_frame,
            // and snapping the current frame values to it.
            //
            // hopefully we have enough player inputs to simulate correctly forward
            //
            comp_history.insert_authoritative(game_clock.frame(), new_comp_val.clone(), &entity);
            // we aren't doing a rollback, since we're updating the current frame:
            *comp = new_comp_val.clone();
        } else {
            // No serversnapshot value for target_frame, better luck next time
            continue;
        }
    }
}

/// when components are removed, we log the death frame
pub(crate) fn record_component_removed_to_alive_ranges<T: Component + Clone + std::fmt::Debug>(
    mut removed: RemovedComponents<T>,
    mut q: Query<&mut ComponentHistory<T>>,
    game_clock: Res<GameClock>,
) {
    for entity in &mut removed {
        if let Ok(mut ct) = q.get_mut(entity) {
            debug!(
                "{entity:?} Component death @ {:?} {:?}",
                game_clock.frame(),
                std::any::type_name::<T>()
            );
            ct.report_death_at_frame(game_clock.frame());
        }
    }
}

/// Runs when we detect that the [`Rollback`] resource has been added.
/// we wind back the game_clock to the first frame of the rollback range, and set the fixed period
/// to zero so frames don't require elapsed time to tick. (ie, fast forward mode)
pub(crate) fn rollback_initiated(
    mut game_clock: ResMut<GameClock>,
    mut rb: ResMut<Rollback>,
    mut fx: ResMut<FixedTime>,
    mut rb_stats: ResMut<RollbackStats>,
) {
    // save original period for restoration after rollback completion
    rb.original_period = Some(fx.period);
    rb_stats.num_rollbacks += 1;
    debug!("🛼 ROLLBACK RESOURCE ADDED ({}), reseting game clock from {:?} for {:?}, setting period -> 0 for fast fwd.", rb_stats.num_rollbacks, game_clock.frame(), rb);
    // make fixed-update ticks free, ie fast-forward the simulation at max speed
    fx.period = Duration::ZERO;
    game_clock.set(rb.range.start);
}

/// during rollback, need to re-insert components that were removed, based on stored lifetimes.
pub(crate) fn reinsert_components_removed_during_rollback_at_correct_frame<
    T: Component + Clone + std::fmt::Debug,
>(
    q: Query<(Entity, &ComponentHistory<T>), Without<T>>,
    game_clock: Res<GameClock>,
    mut commands: Commands,
) {
    // info!(
    //     "reinsert_components_removed_during_rollback_at_correct_frame {:?} {:?}",
    //     game_clock.frame(),
    //     std::any::type_name::<T>()
    // );
    for (entity, comp_history) in q.iter() {
        if comp_history.alive_at_frame(game_clock.frame()) {
            let comp_val = comp_history
                .at_frame(game_clock.frame())
                .unwrap_or_else(|| {
                    panic!(
                        "{entity:?} no comp history @ {:?} for {:?}",
                        game_clock.frame(),
                        std::any::type_name::<T>()
                    )
                });
            trace!(
                "Reinserting {entity:?} -> {:?} during rollback @ {:?}\n{:?}",
                std::any::type_name::<T>(),
                game_clock.frame(),
                comp_val
            );
            commands.entity(entity).insert(comp_val.clone());
        } else {
            trace!(
                "comp not alive at this frame for {entity:?} {:?}",
                comp_history.alive_ranges
            );
        }
    }
}

// during rollback, need to re-remove components that were inserted, based on stored lifetimes.
pub(crate) fn reremove_components_inserted_during_rollback_at_correct_frame<
    T: Component + Clone + std::fmt::Debug,
>(
    mut q: Query<(Entity, &mut ComponentHistory<T>), With<T>>,
    game_clock: Res<GameClock>,
    mut commands: Commands,
) {
    // info!(
    //     "reremove_components_inserted_during_rollback_at_correct_frame {:?} {:?}",
    //     game_clock.frame(),
    //     std::any::type_name::<T>()
    // );
    for (entity, mut comp_history) in q.iter_mut() {
        if !comp_history.alive_at_frame(game_clock.frame()) {
            trace!(
                "Re-removing {entity:?} -> {:?} during rollback @ {:?}",
                std::any::type_name::<T>(),
                game_clock.frame()
            );
            comp_history.remove_frame_and_beyond(game_clock.frame());
            commands.entity(entity).remove::<T>();
        } else {
            trace!("comp_history: {:?}", comp_history.alive_ranges);
        }
    }
}

/// Runs on first frame of rollback, needs to restore the actual component values to our record of
/// them at that frame. This means grabbing the old value from ComponentHistory.
///
/// Also has to handle situation where the component didn't exist at the target frame
/// or it did exist, but doesnt in the present.
pub(crate) fn rollback_initiated_for_component<T: Component + Clone + std::fmt::Debug>(
    rb: Res<Rollback>,
    // T is None in case where component removed but ComponentHistory persists
    mut q: Query<(Entity, Option<&mut T>, &ComponentHistory<T>), Without<NotRollbackable>>,
    mut commands: Commands,
) {
    let target_frame = rb.range.start;
    let verbose = false; //std::any::type_name::<T>() == "bevy_xpbd_2d::components::Position";
    for (entity, opt_c, ct) in q.iter_mut() {
        let str = format!(
            "ROLLBACK {entity:?} {:?} -> {target_frame}",
            std::any::type_name::<T>()
        );
        if !ct.alive_at_frame(target_frame) && opt_c.is_none() {
            // info!("{str}\n(dead in present and rollback target, do nothing)");
            // not alive then, not alive now, do nothing.
            continue;
        }
        if !ct.alive_at_frame(target_frame) && opt_c.is_some() {
            // not alive then, alive now = remove the component
            if verbose {
                info!("{str}\n- Not alive in past, but alive in pressent = remove component. alive ranges = {:?}", ct.alive_ranges);
            }
            commands.entity(entity).remove::<T>();
            continue;
        }
        if ct.alive_at_frame(target_frame) {
            // we actually don't care if the component presently exists or not,
            // since it was alive at target-frame, we re insert with old values.

            // TODO greatest of target_frame and birth_frame so we don't miss respawning?

            if let Some(component) = ct.at_frame(target_frame) {
                // if std::any::type_name::<T>() == "bevy_xpbd_2d::components::PreviousPosition" {
                //     warn!("SeqBuf dump for prevpos: {:?}", ct.values);
                // }
                if let Some(mut current_component) = opt_c {
                    if verbose {
                        info!(
                            "{str}\n- Injecting older data by assigning, {:?} ----> {:?}",
                            Some(current_component.clone()),
                            component
                        );
                    }
                    *current_component = component.clone();
                } else {
                    if verbose {
                        info!(
                            "{str}\n- Injecting older data by reinserting comp, {:?} ----> {:?}",
                            opt_c, component
                        );
                    }
                    commands.entity(entity).insert(component.clone());
                }
            } else {
                // when spawning in other players sometimes this happens.
                // they are despawned by a rollback and can't readd.
                // maybe comps not recorded, or maybe not at correct frame or something.
                warn!(
                    "{str}\n- Need to revive/update component, but not in history @ {target_frame}"
                );
            }
            continue;
        }
        unreachable!("{str} should not get here when restoring component values");
    }
}

/// If we reached the end of the Rollback range, restore the frame period and cleanup.
/// this will remove the [`Rollback`] resource.
pub(crate) fn check_for_rollback_completion(
    game_clock: Res<GameClock>,
    rb: Res<Rollback>,
    mut commands: Commands,
    mut fx: ResMut<FixedTime>,
) {
    // info!("🛼 {:?}..{:?} f={:?} (in rollback)", rb.range.start, rb.range.end, game_clock.frame());
    if rb.range.contains(&game_clock.frame()) {
        return;
    }
    debug!("🛼🛼 Rollback complete. {:?}, resetting period", rb);
    fx.period = rb.original_period.unwrap(); // Duration::from_secs_f32(1./60.);
    commands.remove_resource::<Rollback>();
}

/// despawn marker means remove all useful components, pending actual despawn after
/// ROLLBACK_WINDOW frames have elapsed.
pub(crate) fn process_freshly_added_despawn_markers<T: Component + Clone + std::fmt::Debug>(
    mut q: Query<
        (Entity, &mut ComponentHistory<T>, &mut DespawnMarker),
        (Added<DespawnMarker>, With<T>),
    >,
    mut commands: Commands,
    game_clock: Res<GameClock>,
) {
    for (entity, mut ct, mut despawn_marker) in q.iter_mut() {
        debug!(
            "doing despawn marker component removal for {entity:?} / {:?}",
            std::any::type_name::<T>()
        );
        // make sure despawn marker has current frame, so we know when to actually despawn
        if despawn_marker.0.is_none() {
            despawn_marker.0 = Some(game_clock.frame());
        }
        ct.report_death_at_frame(game_clock.frame());
        commands.entity(entity).remove::<T>();
    }
}

/// Once a [`DespawnMarker`] has been around for `rollback_frames`, do the actual despawn.
pub(crate) fn do_actual_despawn_after_rollback_frames_from_despawn_marker(
    q: Query<(Entity, &DespawnMarker)>,
    mut commands: Commands,
    game_clock: Res<GameClock>,
    timewarp_config: Res<TimewarpConfig>,
) {
    for (entity, marker) in q.iter() {
        if (marker.0.expect("Despawn marker should have a frame!")
            + timewarp_config.rollback_window)
            == game_clock.frame()
        {
            debug!(
                "Doing actual despawn of {entity:?} at frame {:?}",
                game_clock.frame()
            );
            commands.entity(entity).despawn_recursive();
        }
    }
}
