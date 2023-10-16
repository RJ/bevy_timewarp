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
