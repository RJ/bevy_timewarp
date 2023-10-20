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
    mut fx: ResMut<FixedTime>,
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
    rb.original_period = Some(fx.period);
    rb_stats.num_rollbacks += 1;
    let depth = rb.range.end - rb.range.start + 1;
    // we wind clock back 1 past first resim frame, so we can load in data for the frame prior
    // so we go into our first resim frame with components in the correct state.
    let reset_game_clock_to = rb.range.start.saturating_sub(1);
    info!("🛼 ROLLBACK RESOURCE ADDED (rb#{} depth={depth}), reseting game clock from {game_clock:?}-->{reset_game_clock_to} rb:{rb:?}", 
                rb_stats.num_rollbacks);
    // make fixed-update ticks free, ie fast-forward the simulation at max speed
    fx.period = Duration::ZERO;
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
    mut q: Query<(Entity, Option<&mut T>, &ComponentHistory<T>), Without<NoRollback>>,
    mut commands: Commands,
    game_clock: Res<GameClock>,
) {
    for (entity, opt_comp, comp_hist) in q.iter_mut() {
        let rollback_frame = **game_clock;
        let end_frame = rb.range.end;

        let prefix = if rollback_frame != **game_clock {
            warn!(
                "😬 rollback_component {entity:?} {game_clock:?} rollback_frame:{rollback_frame} {}",
                comp_hist.type_name()
            );
            "😬"
        } else {
            ""
        };
        trace!("rollback_component {entity:?} {} rollback-frame:{rollback_frame} {game_clock:?} end_frame={end_frame} {rb:?}", comp_hist.type_name());
        let provenance = match (
            comp_hist.alive_at_frame(rollback_frame),
            comp_hist.alive_at_frame(end_frame),
        ) {
            (true, true) => Provenance::AliveThenAlive,
            (true, false) => Provenance::AliveThenDead,
            (false, true) => Provenance::DeadThenAlive,
            (false, false) => Provenance::DeadThenDead,
        };

        trace!(
            "⛳️ {prefix} {entity:?} {} CH alive_ranges: {:?}",
            comp_hist.type_name(),
            comp_hist.alive_ranges
        );

        match provenance {
            Provenance::DeadThenDead => {
                trace!(
                    "{prefix} {game_clock:?} rollback component {entity:?} {} {provenance:?} - NOOP {:?}",
                    comp_hist.type_name(),
                    comp_hist.alive_ranges
                );
            }
            Provenance::DeadThenAlive => {
                trace!(
                    "{prefix} {game_clock:?} rollback component {entity:?} {} {provenance:?} - REMOVE<T>",
                    comp_hist.type_name()
                );
                commands.entity(entity).remove::<T>();
            }
            Provenance::AliveThenAlive => {
                // TODO we might want a general way to check the oldest frame for this comp,
                // and if we dont have the requested frame, use the oldest instead?
                // assuming a request OLDER than the requested can't be serviced.
                let comp_at_frame = comp_hist.at_frame(rollback_frame);

                // debugging
                if comp_at_frame.is_none() {
                    let oldest_frame = comp_hist.values.oldest_frame();

                    error!(
                        "HMMMM {entity:?} f @ oldest_frame ({oldest_frame}) comp_val = {:?}",
                        comp_hist.at_frame(oldest_frame)
                    );
                    error!("HMMMM {entity:?} {game_clock:?} OPT_COMP = {opt_comp:?}");
                    for f in (rollback_frame - 2)..=(rollback_frame + 2) {
                        error!(
                            "HMMMM {entity:?} f={f} comp_val = {:?}",
                            comp_hist.at_frame(f)
                        );
                    }

                    panic!("{prefix} {game_clock:?} {entity:?} {provenance:?} {} rollback_frame: {rollback_frame} alive_ranges:{:?} rb:{rb:?} oldest value in comp_hist: {oldest_frame} occ:{:?}\n",
                            comp_hist.type_name(), comp_hist.alive_ranges, comp_hist.values.frame_occupancy());
                }
                //
                let comp_val = comp_at_frame.unwrap().clone();
                trace!(
                    "{prefix} {game_clock:?} rollback component {entity:?} {} {provenance:?} - REPLACE WITH {comp_val:?}",
                    comp_hist.type_name()
                );
                if let Some(mut comp) = opt_comp {
                    *comp = comp_val;
                } else {
                    // during new spawns this happens. not a bug.
                    trace!(
                        "{prefix} {entity:?} Actually having to insert for {comp_val:?} doesn't exist yet"
                    );
                    commands.entity(entity).insert(comp_val);
                }
            }
            Provenance::AliveThenDead => {
                let comp_at_frame = comp_hist.at_frame(rollback_frame);
                // debugging
                if comp_at_frame.is_none() {
                    panic!("{game_clock:?} {entity:?} {provenance:?} {} rollback_frame: {rollback_frame} alive_ranges:{:?} rb:{rb:?}",
                            comp_hist.type_name(), comp_hist.alive_ranges);
                }
                //
                let comp_val = comp_at_frame.unwrap().clone();
                trace!(
                    "{prefix} {game_clock:?} rollback component {entity:?} {} {provenance:?} - INSERT {comp_val:?}",
                    comp_hist.type_name()
                );
                commands.entity(entity).insert(comp_val);
            }
        }
    }
}
