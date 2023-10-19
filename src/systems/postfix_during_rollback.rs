use crate::prelude::*;
use bevy::prelude::*;
/*
    Postfix Sets

    NOTE: Timewarp Postfix Systems run AFTER physics.
*/

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

// during rollback, need to re-remove components that were inserted, based on stored lifetimes.
pub(crate) fn rekill_components_during_rollback<T: TimewarpComponent>(
    mut q: Query<(Entity, &mut ComponentHistory<T>), With<T>>,
    game_clock: Res<GameClock>,
    mut commands: Commands,
) {
    let target_frame = game_clock.frame();
    for (entity, mut comp_history) in q.iter_mut() {
        trace!(
            "rekill check? {entity:?} CH {} alive_range: {:?}",
            comp_history.type_name(),
            comp_history.alive_ranges
        );
        if !comp_history.alive_at_frame(target_frame) {
            debug!(
                "Re-removing {entity:?} -> {:?} during rollback for {:?}",
                std::any::type_name::<T>(),
                target_frame
            );
            commands.entity(entity).remove::<T>();
        }
    }
}
