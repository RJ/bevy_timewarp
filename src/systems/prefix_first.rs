/*
    NOTE: Timewarp Prefix Systems run at the top of FixedUpdate:
        * RIGHT BEFORE THE GameClock IS INCREMENTED.
        * Before the game simulation loop
        * Before Physics

*/
use crate::prelude::*;
use bevy::prelude::*;

/// for when we add the ComponentHistory via a trait on EntityMut which doesn't know the error reporting setting
pub(crate) fn enable_error_correction_for_new_component_histories<T: TimewarpComponent>(
    mut q: Query<&mut ComponentHistory<T>, Added<ServerSnapshot<T>>>,
) {
    for mut ch in q.iter_mut() {
        ch.enable_correction_logging();
    }
}

/// when components are removed, we log the death frame
pub(crate) fn record_component_death<T: TimewarpComponent>(
    mut removed: RemovedComponents<T>,
    mut q: Query<&mut ComponentHistory<T>, Without<NoRollback>>,
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
