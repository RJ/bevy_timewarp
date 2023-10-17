use crate::prelude::*;
use bevy::prelude::*;
/*
    NOTE: Timewarp Prefix Systems run at the top of FixedUpdate:
        * RIGHT BEFORE THE GameClock IS INCREMENTED.
        * Before the game simulation loop
        * Before Physics

*/

/// Blueprint components stay wrapped up until their target frame, then we unwrap them
/// so the assembly systems can decorate them with various other components at that frame.
pub(crate) fn unwrap_blueprints_at_target_frame<T: TimewarpComponent>(
    q: Query<(Entity, &AssembleBlueprintAtFrame<T>)>,
    mut commands: Commands,
    game_clock: Res<GameClock>,
    rb: Option<Res<Rollback>>,
) {
    for (e, abaf) in q.iter() {
        // abaf.frame = 10
        // gc (preup) = 9
        // ticks to 10, and we want to assemble this frame.
        // so unpack when pre gc + 1 == abaf.frame.
        // this runs in prefix, before the fixed game clock increments
        if abaf.frame != (1 + **game_clock) {
            debug!("Not assembling, gc={game_clock:?} {abaf:?}");
            continue;
        }
        debug!(
            "🎁 {game_clock:?} Unwrapping {abaf:?} @ {game_clock:?} (for +1 once we enter fixed loop) rb:{rb:?} {}",
            std::any::type_name::<T>()
        );
        commands
            .entity(e)
            .insert(abaf.component.clone())
            .remove::<AssembleBlueprintAtFrame<T>>();
    }
}
