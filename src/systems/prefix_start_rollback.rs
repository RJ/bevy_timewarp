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
    mut commands: Commands,
) {
    // if we're trying to roll back further than our configured rollback window,
    // all sorts of things will fail spectacularly, so i'm just going to panic for now.
    // i think the way to handle this is in the game, if you get an update from the past older
    // than the window that you can't afford to ignore, like a reliable spawn message, then
    // perhaps modify the spawn frame to the oldest allowable frame within the window,
    // and rely on snapshots to sort you out.
    if rb.range.end - rb.range.start > timewarp_config.rollback_window {
        error!(
            "⚠️⚠️⚠️ Attempted to rollback further than rollback_window: {rb:?} @ {:?}",
            game_clock.frame()
        );
        error!("⚠️⚠️⚠️ Ignoring this rollback request. 🛼 ");
        // TODO this isn't really safe - what if there was an ICAF or ABAF and then it never
        // gets unpacked because it was outside the window.
        // perhaps we need to mark the RB as "desperate", rollback to the oldest frame,
        // and unpack anything destined for an even older (oob) frame that go around.
        // at least unpack the ABAF ones, maybe don't care about SS in a desperate rollback.
        rb.abort();
        commands.remove_resource::<Rollback>();
        return;
    }
    // save original period for restoration after rollback completion
    rb.original_period = Some(fx.period);
    rb_stats.num_rollbacks += 1;
    debug!("🛼 ROLLBACK RESOURCE ADDED ({}), reseting game clock from {:?} for {:?}, setting period -> 0 for fast fwd.", rb_stats.num_rollbacks, game_clock.frame(), rb);
    // make fixed-update ticks free, ie fast-forward the simulation at max speed
    fx.period = Duration::ZERO;
    // the start of the rb range is the frame with the newly added authoritative data.
    // since increment happens after the timewarp prefix sets, we set the clock to this value,
    // knowing that it will immediately be incremented to the next frame we need to simulate.
    // (once we've loaded in historical component values)
    game_clock.set(rb.range.start);
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
    mut q: Query<(Entity, Option<&mut T>, &ComponentHistory<T>), Without<NotRollbackable>>,
    mut commands: Commands,
    game_clock: Res<GameClock>,
) {
    if rb.aborted() {
        return;
    }
    let rollback_frame = rb.range.start;

    for (entity, opt_comp, comp_hist) in q.iter_mut() {
        assert_eq!(
            game_clock.frame(),
            rollback_frame,
            "game clock should already be set back by rollback_initiated"
        );

        let opt_comp_at_frame = comp_hist.at_frame(rollback_frame);

        let provenance = match (opt_comp_at_frame.is_some(), opt_comp.is_some()) {
            (true, true) => Provenance::AliveThenAlive,
            (true, false) => Provenance::AliveThenDead,
            (false, true) => Provenance::DeadThenAlive,
            (false, false) => Provenance::DeadThenDead,
        };

        info!(
            "{game_clock:?} rollback component {} {provenance:?}",
            comp_hist.type_name()
        );

        match provenance {
            Provenance::DeadThenDead => {
                // noop
            }
            Provenance::DeadThenAlive => {
                commands.entity(entity).remove::<T>();
            }
            Provenance::AliveThenAlive => {
                *opt_comp.unwrap() = opt_comp_at_frame.unwrap().clone();
            }
            Provenance::AliveThenDead => {
                commands
                    .entity(entity)
                    .insert(opt_comp_at_frame.unwrap().clone());
            }
        }
    }
}
