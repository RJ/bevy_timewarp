use crate::prelude::*;
use bevy::prelude::*;
/*
    Postfix Sets

    NOTE: Timewarp Postfix Systems run AFTER physics.
*/

/// despawn marker means remove all useful components, pending actual despawn after
/// ROLLBACK_WINDOW frames have elapsed.
pub(crate) fn remove_components_from_despawning_entities<T: TimewarpComponent>(
    q: Query<(Entity, &ComponentHistory<T>), (Added<DespawnMarker>, With<T>)>,
    mut commands: Commands,
) {
    for (entity, _ch) in q.iter() {
        debug!(
            "doing despawn marker component removal for {entity:?} / {:?}",
            std::any::type_name::<T>()
        );
        // record_component_death looks at RemovedComponents and will catch this, and
        // register the death (ie, comphist.report_death_at_frame)
        //
        // actually with no recorded history at a frame, that just implies death right?
        // so why do we care about recording death..
        commands.entity(entity).remove::<T>();
    }
}

pub(crate) fn remove_descendents_from_despawning_entities(
    q: Query<Entity, Added<DespawnMarker>>,
    mut commands: Commands,
) {
    for entity in q.iter() {
        info!("Despawn descendants of {entity:?} due to added despawn marker");
        commands.entity(entity).despawn_descendants();
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
                            info!(
                                "Generating Correction for {entity:?}", //old:{:?} new{:?}",
                                                                        // old_val, comp
                            );
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
                        // trace!("End of rb range, but no existing comp to correct");
                        // this is normal in the case of spawning a new entity in the past,
                        // like a bullet. it was never simulated for the current frame yet, so
                        // it's expected that there wasn't an existing comp history val to replace.
                    }
                }
            }
        }
        // the main point of this system is just to save the component value to the buffer:
        // insert() does some logging
        comp_hist.insert(game_clock.frame(), comp.clone(), &entity);
    }
}

/// add the ComponentHistory<T> and ServerSnapshot<T> whenever an entity gets the T component.
/// NB: you must have called `app.register_rollback::<T>()` for this to work.
pub(crate) fn add_timewarp_components<T: TimewarpComponent, const CORRECTION_LOGGING: bool>(
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
        debug!(
            "Adding ComponentHistory<> to {e:?} for {:?}\nInitial val @ {:?} = {:?}",
            std::any::type_name::<T>(),
            game_clock.frame(),
            comp.clone(),
        );
        comp_history.insert(game_clock.frame(), comp.clone(), &e);
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
/// TODO not sure if we need this birth tracking at all?
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
        // if an entity was created with InsertComponentAtPastFrame it will have its birth frame recorded
        // but this system won't catch it via Added<> until the next frame, which we need to supress.
        // an alternative solution is to force insert_components_at_prior_frames to run before this,
        // with an apply_deferred, but that seemed worse. systems run in parallel at the mo.
        if ct.alive_ranges.first() == Some(&(game_clock.frame() - 1, None)) {
            // this comp is alive and born last frame: skip registering the birth.
            return;
        }
        ct.report_birth_at_frame(game_clock.frame());
    }
}
