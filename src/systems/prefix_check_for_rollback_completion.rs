use crate::prelude::*;
use bevy::prelude::*;
/*
    NOTE: Timewarp Prefix Systems run at the top of FixedUpdate:
        * RIGHT BEFORE THE GameClock IS INCREMENTED.
        * Before the game simulation loop
        * Before Physics

*/

/// If we reached the end of the Rollback range, restore the frame period and cleanup.
/// this will remove the [`Rollback`] resource.
pub(crate) fn check_for_rollback_completion(
    game_clock: Res<GameClock>,
    rb: Res<Rollback>,
    mut commands: Commands,
    mut fx: ResMut<FixedTime>,
) {
    if rb.range.end != **game_clock {
        return;
    }
    // we keep track of the previous rollback mainly for integration tests
    commands.insert_resource(PreviousRollback(rb.as_ref().clone()));
    info!(
        "🛼🛼 Rollback complete. {:?}, frames: {} gc:{game_clock:?}",
        rb,
        rb.range.end - rb.range.start
    );
    fx.period = rb.original_period.unwrap();
    commands.remove_resource::<Rollback>();
}
