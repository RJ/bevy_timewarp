/*
    NOTE: Timewarp Prefix Systems run at the top of FixedUpdate:
        * RIGHT BEFORE THE GameClock IS INCREMENTED.
        * Before the game simulation loop
        * Before Physics

*/
use crate::prelude::*;
use bevy::prelude::*;

pub(crate) fn enable_error_correction_for_new_component_histories<T: TimewarpComponent>(
    mut q: Query<&mut ComponentHistory<T>, Added<ServerSnapshot<T>>>,
) {
    for mut ch in q.iter_mut() {
        ch.enable_correction_logging();
    }
}
