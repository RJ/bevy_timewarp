use crate::prelude::*;
use bevy::prelude::*;
use std::time::Duration;
/*
    NOTE: Timewarp Prefix Systems run at the top of FixedUpdate:
        * RIGHT BEFORE THE GameClock IS INCREMENTED.
        * Before the game simulation loop
        * Before Physics

*/

/// Runs when we detect that the [`Rollback`] resource has been added.
///
/// The start of the rollback
/// we wind back the game_clock to the first frame of the rollback range, and set the fixed period
/// to zero so frames don't require elapsed time to tick. (ie, fast forward mode)
pub(crate) fn rollback_initiated(
    mut game_clock: ResMut<GameClock>,
    mut rb: ResMut<Rollback>,
    mut fx: ResMut<Time<Fixed>>,
    mut rb_stats: ResMut<RollbackStats>,
    timewarp_config: Res<TimewarpConfig>,
) {
    // if we're trying to roll back further than our configured rollback window,
    // all sorts of things will fail spectacularly, so i'm just going to panic for now.
    // i think the way to handle this is in the game, if you get an update from the past older
    // than the window that you can't afford to ignore, like a reliable spawn message, then
    // deal with it and don't tell timewarp.
    if rb.range.end - rb.range.start >= timewarp_config.rollback_window {
        panic!(
            "⚠️⚠️⚠️ Attempted to rollback further than rollback_window: {rb:?} @ {:?}",
            game_clock.frame()
        );
    }
    // save original period for restoration after rollback completion
    rb.original_period = Some(fx.timestep());
    rb_stats.num_rollbacks += 1;
    let depth = rb.range.end - rb.range.start + 1;
    // we wind clock back 1 past first resim frame, so we can load in data for the frame prior
    // so we go into our first resim frame with components in the correct state.
    let reset_game_clock_to = rb.range.start.saturating_sub(1);
    info!("🛼 ROLLBACK RESOURCE ADDED (rb#{} depth={depth}), reseting game clock from {game_clock:?}-->{reset_game_clock_to} rb:{rb:?}", 
                rb_stats.num_rollbacks);
    // make fixed-update ticks free, ie fast-forward the simulation at max speed
		// ideally this is zero, but this function panics if we try to set it to zero
		// as of bevy 0.12
    fx.set_timestep(Duration::from_nanos(1));
    // the start of the rb range is the frame with the newly added authoritative data.
    // since increment happens after the timewarp prefix sets, we set the clock to this value - 1,
    // knowing that it will immediately be incremented to the next frame we need to simulate.
    // (once we've loaded in historical component values)
    game_clock.set(reset_game_clock_to);
}

// for clarity when rolling back components
#[derive(Debug)]
enum Provenance {
    AliveThenAlive,
    AliveThenDead,
    DeadThenAlive,
    DeadThenDead,
}

/// Runs if Rollback was only just Added.
/// A rollback range starts on the frame we added new authoritative data for, so we need to
/// restore component values to what they were at that frame, so the next frame can be resimulated.
///
/// Also has to handle situation where the component didn't exist then, or it did exist, but doesnt in the present.
pub(crate) fn rollback_component<T: TimewarpComponent>(
    rb: Res<Rollback>,
    // T is None in case where component removed but ComponentHistory persists
    mut q: Query<
        (
            Entity,
            Option<&mut T>,
            &ComponentHistory<T>,
            &ServerSnapshot<T>,
        ),
        Without<NoRollback>,
    >,
    mut commands: Commands,
    game_clock: Res<GameClock>,
) {
    for (entity, opt_comp, ch, ss) in q.iter_mut() {
        let rollback_frame = **game_clock;
        let end_frame = rb.range.end;

        trace!("rollback_component {entity:?} {} rollback-frame:{rollback_frame} {game_clock:?} end_frame={end_frame} {rb:?}", ch.type_name());

        // comp@frame is cloned once here
        //
        // in cases where just-in-time component values are delivered by replicon, inserted,
        // and a rollback is triggered, the value can end up being in the SS but not ever written
        // to the CH, because we never reached the TW postfix sets that frame.
        //
        // we always prefer the SS value if available, otherwise our own record from the CH.
        let comp_at_rollback_frame = match ss.at_frame(rollback_frame) {
            Some(val) => Some(val.clone()),
            None => ch.at_frame(rollback_frame).cloned(),
        };

        let provenance = match (
            comp_at_rollback_frame.is_some(),
            ch.alive_at_frame(end_frame),
        ) {
            (true, true) => Provenance::AliveThenAlive,
            (true, false) => Provenance::AliveThenDead,
            (false, true) => Provenance::DeadThenAlive,
            (false, false) => Provenance::DeadThenDead,
        };

        trace!(
            "⛳️ {entity:?} {} CH alive_ranges: {:?}",
            ch.type_name(),
            ch.alive_ranges
        );

        match provenance {
            Provenance::DeadThenDead => {
                trace!(
                    "{game_clock:?} rollback component {entity:?} {} {provenance:?} - NOOP {:?}",
                    ch.type_name(),
                    ch.alive_ranges
                );
            }
            Provenance::DeadThenAlive => {
                trace!(
                    "{game_clock:?} rollback component {entity:?} {} {provenance:?} - REMOVE<T>",
                    ch.type_name()
                );
                commands.entity(entity).remove::<T>();
            }
            Provenance::AliveThenAlive => {
                trace!(
                    "{game_clock:?} rollback component {entity:?} {} {provenance:?} - REPLACE WITH {comp_at_rollback_frame:?}",
                    ch.type_name()
                );
                if let Some(mut comp) = opt_comp {
                    *comp = comp_at_rollback_frame.expect("Component should be alive here!");
                } else {
                    // during new spawns this happens. not a bug.
                    commands
                        .entity(entity)
                        .insert(comp_at_rollback_frame.expect("Component should be alive here"));
                }
            }
            Provenance::AliveThenDead => {
                trace!(
                    "{game_clock:?} rollback component {entity:?} {} {provenance:?} - INSERT {comp_at_rollback_frame:?}",
                    ch.type_name()
                );
                commands
                    .entity(entity)
                    .insert(comp_at_rollback_frame.expect("Component should be alive here!!"));
            }
        }
    }
}
