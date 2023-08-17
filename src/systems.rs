use crate::prelude::*;
use bevy::prelude::*;
use std::time::Duration;

/// wipes RemovedComponents<T> queue for component T.
/// useful during rollback, because we don't react to removals that are part of resimulating.
pub(crate) fn clear_removed_components_queue<T: Component>(
    mut e: RemovedComponents<T>,
    game_clock: Res<GameClock>,
) {
    if !e.is_empty() {
        info!(
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
pub(crate) fn add_timeline_versions_of_component<T: Component + Clone + std::fmt::Debug>(
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
        comp_history.insert(game_clock.frame(), comp.clone());

        info!(
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
/// won't be called first time comp is added, since it won't have a ComponentTimeline yet.
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
        info!(
            "{entity:?} Component birth @ {:?} {:?}",
            game_clock.frame(),
            std::any::type_name::<T>()
        );
        ct.report_birth_at_frame(game_clock.frame());
    }
}

/// Write current value of component to the ComponentHistory buffer for this frame
pub(crate) fn record_local_timeline_values<T: Component + Clone + std::fmt::Debug>(
    mut q: Query<(Entity, &T, &mut ComponentHistory<T>)>,
    game_clock: Res<GameClock>,
) {
    for (_entity, comp, mut comp_timeline) in q.iter_mut() {
        // info!("Storing to timeline for {_entity:?} @ {:?} = {:?}", game_clock.frame(), comp);
        comp_timeline.insert(game_clock.frame(), comp.clone());
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
    mut opt_rb: Option<ResMut<Rollback>>,
    game_clock: Res<GameClock>,
) {
    for (entity, icaf, opt_ch, opt_ss) in q.iter_mut() {
        warn!("{icaf:?}");
        let mut ent_cmd = commands.entity(entity);
        ent_cmd.remove::<InsertComponentAtFrame<T>>();

        // if the entity never had this component type T before, we'll need to insert
        // the ComponentHistory and ServerSnapshot components.
        // If they already exist, just insert at the correct frame.
        if let Some(mut ch) = opt_ch {
            ch.insert_authoritative(icaf.frame, icaf.component.clone());
            ch.report_birth_at_frame(icaf.frame);
            info!("Inserting component at past frame for existing ComponentHistory");
        } else {
            let mut ch = ComponentHistory::<T>::with_capacity(
                timewarp_config.rollback_window as usize,
                icaf.frame,
            );
            ch.insert_authoritative(icaf.frame, icaf.component.clone());
            ent_cmd.insert(ch);
            info!("Inserting component at past frame by inserting new ComponentHistory");
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
        if let Some(ref mut rb) = opt_rb {
            // this might extend an existing rollback range
            rb.ensure_start_covers(icaf.frame);
        } else {
            commands.insert_resource(Rollback::new(icaf.frame, game_clock.frame()));
        }
    }
}

/// - Has the ServerSnapshot changed?
/// - Does it contain a snapshot newer than the last authoritative frame in the component history?
/// - Does the snapshot value at that frame differ from the predicted values we used?
/// - If so, copy the snapshot value to ComponentHistory and trigger a rollback to that frame.
pub(crate) fn apply_new_snapshot_values_to_timeline_and_trigger_rollback<
    T: Component + Clone + std::fmt::Debug,
>(
    mut q: Query<
        (
            &mut ComponentHistory<T>,
            &ServerSnapshot<T>,
            Option<&Anachronous>,
        ),
        Changed<ServerSnapshot<T>>,
    >,
    game_clock: Res<GameClock>,
    mut opt_rb: Option<ResMut<Rollback>>,
    mut commands: Commands,
) {
    for (mut comp_timeline, comp_server, opt_anach) in q.iter_mut() {
        // warn!("apply_new_snapshot_values_to_timeline_and_trigger_rollback triggered");
        // if the server snapshot component has been updated, and contains a newer authoritative
        // value than what we've already applied, we might need to rollback and resim.
        if comp_server.values.sequence() == 0 {
            // no data yet
            continue;
        }
        let new_snapshot_frame = comp_server.values.sequence() - 1;
        if comp_timeline.most_recent_authoritative_frame < new_snapshot_frame {
            // info!("QQQ new_snapshot_frame: {new_snapshot_frame} comp-timeline.most_rec={:?}",
            //     comp_timeline.most_recent_authoritative_frame
            // );
            let new_comp_val = if let Some(anach) = opt_anach {
                info!("ANACH COMP");
                // anachronous entities need to be in the past so much load older snapshot data
                // add 10 updates per sec, and 60fps, 3x6 = 18, so should hit an actual value
                // otherwise we'd need to interp between or roll to nearest snapshot?
                let target_frame = new_snapshot_frame - anach.frames_behind;
                if let Some(v) = comp_server.values.get(target_frame) {
                    v.clone()
                } else {
                    // search for snapshot values
                    let mut f = new_snapshot_frame;
                    for _ in 1..60 {
                        error!("@ {f} = {:?}", comp_server.values.get(f));
                        if f == 0 {
                            break;
                        }
                        f -= 1;
                    }
                    error!("No snapshot for target frame: {target_frame}, new_snapshot_frame: {new_snapshot_frame}");
                    continue;
                }
            } else {
                comp_server.values.get(new_snapshot_frame).unwrap().clone()
            };
            // copy from server snapshot to component history. in prep for rollback
            // TODO check if local predicted value matches snapshot and bypass!!
            comp_timeline.insert_authoritative(new_snapshot_frame, new_comp_val);

            if let Some(ref mut rb) = opt_rb {
                // this might extend an existing rollback range
                rb.ensure_start_covers(new_snapshot_frame);
            } else {
                commands.insert_resource(Rollback::new(new_snapshot_frame, game_clock.frame()));
            }
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
            info!(
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
    info!("🛼 ROLLBACK RESOURCE ADDED ({}), reseting game clock from {:?} for {:?}, setting period -> 0 for fast fwd.", rb_stats.num_rollbacks, game_clock.frame(), rb);
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
    debug!(
        "reinsert_components_removed_during_rollback_at_correct_frame {:?} {:?}",
        game_clock.frame(),
        std::any::type_name::<T>()
    );
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
            debug!(
                "Reinserting {entity:?} -> {:?} during rollback @ {:?}\n{:?}",
                std::any::type_name::<T>(),
                game_clock.frame(),
                comp_val
            );
            commands.entity(entity).insert(comp_val.clone());
        } else {
            debug!(
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
    q: Query<(Entity, &ComponentHistory<T>), With<T>>,
    game_clock: Res<GameClock>,
    mut commands: Commands,
) {
    debug!(
        "reremove_components_inserted_during_rollback_at_correct_frame {:?} {:?}",
        game_clock.frame(),
        std::any::type_name::<T>()
    );
    for (entity, comp_history) in q.iter() {
        if !comp_history.alive_at_frame(game_clock.frame()) {
            debug!(
                "Re-removing {entity:?} -> {:?} during rollback @ {:?}",
                std::any::type_name::<T>(),
                game_clock.frame()
            );
            commands.entity(entity).remove::<T>();
        } else {
            debug!("comp_history: {:?}", comp_history.alive_ranges);
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
    // T is None in case where component removed but timeline persists
    mut q: Query<
        (
            Entity,
            Option<&mut T>,
            &ComponentHistory<T>,
            Option<&Anachronous>,
        ),
        Without<NotRollbackable>,
    >,
    mut commands: Commands,
) {
    let target_frame = rb.range.start;
    let verbose = false; //true; // std::any::type_name::<T>() == "bevy_xpbd_2d::components::Position";
    for (entity, opt_c, ct, m_anach) in q.iter_mut() {
        let str = format!(
            "ROLLBACK {entity:?} {:?} -> {target_frame} {m_anach:?}",
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
                warn!("{str}\n- Need to revive/update component, but not in timeline!");
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
    info!("🛼🛼 Rollback complete. {:?}, resetting period", rb);
    fx.period = rb.original_period.unwrap(); // Duration::from_secs_f32(1./60.);
    commands.remove_resource::<Rollback>();
}

// TODO despawn and removal stuff into a Header set?

/// despawn marker means remove all useful components, pending actual despawn after
/// ROLLBACK_WINDOW frames have elapsed.
pub(crate) fn remove_component_after_despawn_marker_added<
    T: Component + Clone + std::fmt::Debug,
>(
    mut q: Query<(Entity, &mut ComponentHistory<T>), (Added<DespawnMarker>, With<T>)>,
    mut commands: Commands,
    game_clock: Res<GameClock>,
) {
    for (entity, mut ct) in q.iter_mut() {
        info!(
            "doing despawn marker component removal for {entity:?} / {:?}",
            std::any::type_name::<T>()
        );
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
        if (marker.0 + timewarp_config.rollback_window) == game_clock.frame() {
            info!(
                "Doing actual despawn of {entity:?} at frame {:?}",
                game_clock.frame()
            );
            commands.entity(entity).despawn_recursive();
        }
    }
}
