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

/// during rollback, need to re-insert components that were removed, based on stored lifetimes.
pub(crate) fn rebirth_components_during_rollback<T: TimewarpComponent>(
    q: Query<(Entity, &ComponentHistory<T>), Without<T>>,
    game_clock: Res<GameClock>,
    mut commands: Commands,
    rb: Res<Rollback>,
) {
    for (entity, comp_history) in q.iter() {
        let target_frame = game_clock.frame();
        if comp_history.alive_at_frame(target_frame) {
            info!(
                "CHecking if {entity:?} {} alive at {game_clock:?} - YES ",
                comp_history.type_name()
            );
            // we could go fishing in SS for this, but it should be here if its alive.
            // i think i'm only hitting this with rollback underflows though, during load?
            // need more investigation and to figure out a test case..
            let comp_val = comp_history.at_frame(target_frame).unwrap_or_else(|| {
                error!(
                    // gaps in CH values, can't rb to a gap?
                    "{entity:?} no comp history for {:?} for {:?} focc:{:?} {game_clock:?} {rb:?}",
                    target_frame,
                    std::any::type_name::<T>(),
                    comp_history.values.frame_occupancy(),
                );
                error!("alive_ranges: {:?}", comp_history.alive_ranges);
                panic!("death");
            });

            debug!(
                "Reinserting {entity:?} -> {:?} during rollback for {:?}\n{:?}",
                std::any::type_name::<T>(),
                target_frame,
                comp_val
            );
            commands.entity(entity).insert(comp_val.clone());
        } else {
            info!(
                "CHecking if {entity:?} {} alive at {game_clock:?} - NO ",
                comp_history.type_name()
            );
            trace!(
                "comp not alive at {game_clock:?} for {entity:?} {:?} {}",
                comp_history.alive_ranges,
                std::any::type_name::<T>(),
            );
        }
    }
}
