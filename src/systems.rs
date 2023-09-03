use crate::prelude::*;
use bevy::prelude::*;
use std::time::Duration;

/// systems that want to initiate a rollback write one of these to
/// the Events<RollbackRequest> queue.
#[derive(Event, Debug)]
pub struct RollbackRequest(pub FrameNumber);

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
    T: TimewarpComponent,
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
            TimewarpStatus::new(0),
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
pub(crate) fn record_component_birth<T: TimewarpComponent>(
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
pub(crate) fn record_component_history<T: TimewarpComponent>(
    mut q: Query<(
        Entity,
        &T,
        &mut ComponentHistory<T>,
        Option<&mut TimewarpCorrection<T>>,
    )>,
    game_clock: Res<GameClock>,
    mut commands: Commands,
    opt_rb: Option<Res<Rollback>>,
) {
    for (entity, comp, mut comp_hist, opt_correction) in q.iter_mut() {
        // if we're in rollback, and on the last frame, we're about to overwrite something.
        // we need to preserve it an report a misprediction, if it differs from the new value.
        if comp_hist.correction_logging_enabled {
            if let Some(ref rb) = opt_rb {
                if rb.range.end == game_clock.frame() {
                    if let Some(old_val) = comp_hist.at_frame(game_clock.frame()) {
                        if *old_val != *comp {
                            warn!("Generating Correction");
                            if let Some(mut correction) = opt_correction {
                                correction.before = old_val.clone();
                                correction.after = comp.clone();
                                correction.frame = game_clock.frame();
                            } else {
                                commands.entity(entity).insert(TimewarpCorrection::<T> {
                                    before: old_val.clone(),
                                    after: comp.clone(),
                                    frame: game_clock.frame(),
                                });
                            }
                        }
                    } else {
                        warn!("End of rb range, but no existing comp to correct");
                    }
                }
            }
        }
        trace!("record @ {:?} {entity:?} {comp:?}", game_clock.frame());
        // the main point of this system is just to save the component value to the buffer:
        comp_hist.insert(game_clock.frame(), comp.clone(), &entity);
    }
}

/// when you need to insert a component at a previous frame, you wrap it up like:
/// InsertComponentAtFrame::<Shield>::new(frame, shield_component);
/// and this system handles things.
/// not triggering rollbacks here, that will happen if we add or change SS.
pub(crate) fn insert_components_at_prior_frames<T: TimewarpComponent>(
    mut q: Query<
        (
            Entity,
            &InsertComponentAtFrame<T>,
            // NOTE the timewarp components might not have been added if this is a first-timer entity
            // which is why they have to be Option<> here, in case we need to insert them.
            Option<&mut ComponentHistory<T>>,
            Option<&mut ServerSnapshot<T>>,
            Option<&mut TimewarpStatus>,
        ),
        Added<InsertComponentAtFrame<T>>,
    >,
    mut commands: Commands,
    timewarp_config: Res<TimewarpConfig>,
) {
    for (entity, icaf, opt_ch, opt_ss, opt_tw_status) in q.iter_mut() {
        // warn!("{icaf:?}");
        let mut ent_cmd = commands.entity(entity);
        ent_cmd.remove::<InsertComponentAtFrame<T>>();
        if let Some(mut tw_status) = opt_tw_status {
            tw_status.set_snapped_at(icaf.frame);
        } else {
            ent_cmd.insert(TimewarpStatus::new(icaf.frame));
        }
        // if the entity never had this component type T before, we'll need to insert
        // the ComponentHistory and ServerSnapshot components.
        // If they already exist, just insert at the correct frame.
        if let Some(mut ch) = opt_ch {
            ch.insert(icaf.frame, icaf.component.clone(), &entity);
            ch.report_birth_at_frame(icaf.frame);
            trace!("Inserting component at past frame for existing ComponentHistory");
        } else {
            let mut ch = ComponentHistory::<T>::with_capacity(
                timewarp_config.rollback_window as usize,
                icaf.frame,
            );
            ch.insert(icaf.frame, icaf.component.clone(), &entity);
            ent_cmd.insert(ch);
            trace!("Inserting component at past frame by inserting new ComponentHistory");
        }
        // reminder: inserting a new ServerSnapshot, or adding a value to an existing ServerSnapshot
        // will cause a rollback, per the `trigger_rollback_when_snapshot_added` system
        if let Some(mut ss) = opt_ss {
            // Entity already has a ServerSnapshot component, add our new data
            ss.insert(icaf.frame, icaf.component.clone());
        } else {
            // Add a new ServerSnapshot component to the entity
            let mut ss =
                ServerSnapshot::<T>::with_capacity(timewarp_config.rollback_window as usize * 60); // TODO yuk
            ss.insert(icaf.frame, icaf.component.clone());
            ent_cmd.insert(ss);
        }
    }
}

/// If a new snapshot was added, we may need to initiate a rollback to that frame
pub(crate) fn trigger_rollback_when_snapshot_added<T: TimewarpComponent>(
    mut q: Query<
        (
            Entity,
            &ServerSnapshot<T>,
            &mut ComponentHistory<T>,
            &mut TimewarpStatus,
        ),
        Changed<ServerSnapshot<T>>, // this includes Added<>
    >,
    game_clock: Res<GameClock>,
    mut rb_ev: ResMut<Events<RollbackRequest>>,
) {
    for (entity, server_snapshot, mut comp_hist, mut tw_status) in q.iter_mut() {
        let snap_frame = server_snapshot.values.newest_frame();

        if snap_frame == 0 {
            continue;
        }
        // if this snapshot is ahead of where we want the entity to be, it's useless to rollback
        // TODO test if we get a snapshot for the frame we just processed.. what if snap_frame == game_clock.frame()
        // does the value still get applied?
        if snap_frame > game_clock.frame() {
            warn!(
                "f={:?} {entity:?} Snap frame {snap_frame} > f",
                game_clock.frame(),
            );
            continue;
        }
        tw_status.set_snapped_at(snap_frame);
        // insert into comp history, because if we rollback exactly to snap-frame
        // the `apply_snapshot_to_component` won't have run, and we need it in there.
        let comp_from_snapshot = server_snapshot
            .at_frame(snap_frame)
            .expect("snap_frame must have a value here");

        comp_hist.insert(snap_frame, comp_from_snapshot.clone(), &entity);

        debug!("f={:?} SNAPPING and Triggering rollback due to snapshot. {entity:?} snap_frame: {snap_frame}", game_clock.frame());

        // trigger a rollback
        //
        // Although this is the only system that asks for rollbacks, we request them
        // by writing to an Event<> and consolidating afterwards.
        // It's possible different <T: Component> generic versions of this function
        // will want to rollback to different frames, and we can't have them trampling
        // over eachother by just inserting the Rollback resoruce directly.
        rb_ev.send(RollbackRequest(snap_frame));
    }
}

// /// if we are at a frame where a snapshot exists, apply the SS value to the component.
// pub(crate) fn apply_snapshot_to_component_if_available<T: TimewarpComponent>(
//     mut q: Query<(Entity, &mut T, &mut ComponentHistory<T>, &ServerSnapshot<T>)>,
//     game_clock: Res<GameClock>,
// ) {
//     for (entity, mut comp, mut comp_history, comp_server) in q.iter_mut() {
//         if comp_server.values.newest_frame() == 0 {
//             // no data yet
//             continue;
//         }

//         let verbose = true; // std::any::type_name::<T>().contains("::Position");

//         // is there a snapshot value for our target_frame?
//         if let Some(new_comp_val) = comp_server.values.get(game_clock.frame()) {
//             if verbose {
//                 info!(
//                     "🫰 f={:?} SNAPPING for {:?}",
//                     game_clock.frame(),
//                     std::any::type_name::<T>(),
//                 );
//             }
//             comp_history.insert(game_clock.frame(), new_comp_val.clone(), &entity);
//             *comp = new_comp_val.clone();
//         }
//     }
// }

/// when components are removed, we log the death frame
pub(crate) fn record_component_death<T: TimewarpComponent>(
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
pub(crate) fn rebirth_components_during_rollback<T: TimewarpComponent>(
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
pub(crate) fn rekill_components_during_rollback<T: TimewarpComponent>(
    mut q: Query<(Entity, &mut ComponentHistory<T>), With<T>>,
    game_clock: Res<GameClock>,
    mut commands: Commands,
) {
    for (entity, mut comp_history) in q.iter_mut() {
        if !comp_history.alive_at_frame(game_clock.frame()) {
            trace!(
                "Re-removing {entity:?} -> {:?} during rollback @ {:?}",
                std::any::type_name::<T>(),
                game_clock.frame()
            );
            comp_history.remove_frame_and_beyond(game_clock.frame());
            commands.entity(entity).remove::<T>();
        }
    }
}

/// Runs on first frame of rollback, needs to restore the actual component values to our record of
/// them at that frame.
///
/// Also has to handle situation where the component didn't exist at the target frame
/// or it did exist, but doesnt in the present.
///
/// Also note because `rollback_initiated` has already run, the game clock is set to the first
/// rollback frame. So really all we are doing is syncing the actual Components with the values
/// from ComponentHistory at the "current" frame.
pub(crate) fn rollback_component<T: TimewarpComponent>(
    rb: Res<Rollback>,
    // T is None in case where component removed but ComponentHistory persists
    mut q: Query<(Entity, Option<&mut T>, &ComponentHistory<T>), Without<NotRollbackable>>,
    mut commands: Commands,
    game_clock: Res<GameClock>,
) {
    for (entity, opt_comp, comp_hist) in q.iter_mut() {
        let verbose = false;
        let rollback_frame = rb.range.start;
        assert_eq!(
            game_clock.frame(),
            rollback_frame,
            "game clock should already be set back by rollback_initiated"
        );

        let str = format!(
            "ROLLBACK {:?} {entity:?} -> {:?} target rollback_frame={rollback_frame}",
            std::any::type_name::<T>(),
            rb.range,
        );
        if !comp_hist.alive_at_frame(rollback_frame) && opt_comp.is_none() {
            // info!("{str}\n(dead in present and rollback target, do nothing)");
            // not alive then, not alive now, do nothing.
            continue;
        }
        if !comp_hist.alive_at_frame(rollback_frame) && opt_comp.is_some() {
            // not alive then, alive now = remove the component
            if verbose {
                info!("{str}\n- Not alive in past, but alive in pressent = remove component. alive ranges = {:?}", comp_hist.alive_ranges);
            }
            commands.entity(entity).remove::<T>();
            continue;
        }
        if comp_hist.alive_at_frame(rollback_frame) {
            if let Some(component) = comp_hist.at_frame(rollback_frame) {
                if let Some(mut current_component) = opt_comp {
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
                            opt_comp, component
                        );
                    }
                    commands.entity(entity).insert(component.clone());
                }
            } else {
                // we chose to rollback to this frame, we would expect there to be data here..
                error!(
                    "{str}\n- Need to revive/update component, but not in history @ {rollback_frame}. comp_hist range: {:?}", comp_hist.values.current_range()
                );
            }
        }
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
    // we keep track of the previous rollback mainly for integration tests
    commands.insert_resource(PreviousRollback(rb.as_ref().clone()));
    debug!("🛼🛼 Rollback complete. {:?}, resetting period", rb);
    fx.period = rb.original_period.unwrap();
    commands.remove_resource::<Rollback>();
}

/// despawn markers often added using DespawnMarker::new() for convenience, we fill them
/// with the current frame here.
pub(crate) fn add_frame_to_freshly_added_despawn_markers(
    mut q: Query<&mut DespawnMarker, Added<DespawnMarker>>,
    game_clock: Res<GameClock>,
) {
    for mut despawn_marker in q.iter_mut() {
        if despawn_marker.0.is_none() {
            despawn_marker.0 = Some(game_clock.frame());
        }
    }
}

/// despawn marker means remove all useful components, pending actual despawn after
/// ROLLBACK_WINDOW frames have elapsed.
pub(crate) fn remove_components_from_despawning_entities<T: TimewarpComponent>(
    mut q: Query<(Entity, &mut ComponentHistory<T>), (Added<DespawnMarker>, With<T>)>,
    mut commands: Commands,
    game_clock: Res<GameClock>,
) {
    for (entity, mut ct) in q.iter_mut() {
        debug!(
            "doing despawn marker component removal for {entity:?} / {:?}",
            std::any::type_name::<T>()
        );
        ct.report_death_at_frame(game_clock.frame());
        commands.entity(entity).remove::<T>();
    }
}

/// Once a [`DespawnMarker`] has been around for `rollback_frames`, do the actual despawn.
pub(crate) fn despawn_entities_with_elapsed_despawn_marker(
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
