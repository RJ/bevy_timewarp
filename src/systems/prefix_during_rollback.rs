use crate::{game_clock, prelude::*};
use bevy::prelude::*;
/*
    NOTE: Timewarp Prefix Systems run at the top of FixedUpdate:
        * RIGHT BEFORE THE GameClock IS INCREMENTED.
        * Before the game simulation loop
        * Before Physics

*/

/// when components are removed, we log the death frame
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

/// during rollback, need to re-insert components that were removed, based on stored lifetimes.
pub(crate) fn rebirth_components_during_rollback<T: TimewarpComponent>(
    q: Query<(Entity, &ComponentHistory<T>, Option<&OriginFrame>), Without<T>>,
    game_clock: Res<GameClock>,
    mut commands: Commands,
    rb: Res<Rollback>,
) {
    for (entity, comp_history, opt_originframe) in q.iter() {
        let target_frame = game_clock.frame().max(opt_originframe.map_or(0, |of| of.0));
        if comp_history.alive_at_frame(target_frame) {
            let comp_val = comp_history.at_frame(target_frame).unwrap_or_else(|| {
                error!(
                    // hitting this, spamming bullets.. gaps in CH values, can't rb to a gap?
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
            trace!(
                "comp not alive at {game_clock:?} for {entity:?} {:?} {}",
                comp_history.alive_ranges,
                std::any::type_name::<T>(),
            );
        }
    }
}
