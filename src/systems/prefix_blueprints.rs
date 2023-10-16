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
        if abaf.frame != **game_clock {
            info!("Not unwrapping {abaf:?} @ {game_clock:?}");
            continue;
        }
        info!(
            "🎁 Unwrapping {abaf:?} @ {game_clock:?} {rb:?} {}",
            std::any::type_name::<T>()
        );
        commands
            .entity(e)
            .insert(abaf.component.clone())
            .remove::<AssembleBlueprintAtFrame<T>>();
    }
}
