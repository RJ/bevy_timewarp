use crate::prelude::*;
use bevy::prelude::*;
/*
    NOTE: Timewarp Prefix Systems run at the top of FixedUpdate:
        * RIGHT BEFORE THE GameClock IS INCREMENTED.
        * Before the game simulation loop
        * Before Physics

*/

/// when components are removed, we log the death frame - doesnt run in rollback
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

// during rollback, need to re-remove components that were inserted, based on stored lifetimes.
pub(crate) fn rekill_components_during_rollback<T: TimewarpComponent>(
    mut q: Query<(Entity, &mut ComponentHistory<T>), With<T>>,
    game_clock: Res<GameClock>,
    mut commands: Commands,
) {
    for (entity, mut comp_history) in q.iter_mut() {
        if !comp_history.alive_at_frame(game_clock.frame()) {
            if comp_history.correction_logging_enabled {
                debug!(
                    "Re-removing {entity:?} -> {:?} during rollback @ {:?}",
                    std::any::type_name::<T>(),
                    game_clock.frame()
                );
            } else {
                trace!(
                    "Re-removing {entity:?} -> {:?} during rollback @ {:?}",
                    std::any::type_name::<T>(),
                    game_clock.frame()
                );
            }
            comp_history.remove_frame_and_beyond(game_clock.frame());
            commands.entity(entity).remove::<T>();
        }
    }
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
                    error!(
                        // hitting this, spamming bullets
                        "{entity:?} no comp history @ {:?} for {:?}",
                        game_clock.frame(),
                        std::any::type_name::<T>()
                    );
                    error!("alive_ranges: {:?}", comp_history.alive_ranges);
                    panic!("death");
                });
            if comp_history.correction_logging_enabled {
                debug!(
                    "Reinserting {entity:?} -> {:?} during rollback @ {:?}\n{:?}",
                    std::any::type_name::<T>(),
                    game_clock.frame(),
                    comp_val
                );
            } else {
                trace!(
                    "Reinserting {entity:?} -> {:?} during rollback @ {:?}\n{:?}",
                    std::any::type_name::<T>(),
                    game_clock.frame(),
                    comp_val
                );
            }
            commands.entity(entity).insert(comp_val.clone());
        } else {
            trace!(
                "comp not alive at {game_clock:?} for {entity:?} {:?} {}",
                comp_history.alive_ranges,
                std::any::type_name::<T>(),
            );
        }
    }
}
