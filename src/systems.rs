use crate::prelude::*;
use bevy::prelude::*;

pub(crate) mod postfix_components;
pub(crate) mod postfix_during_rollback;
pub(crate) mod postfix_last;

pub(crate) mod prefix_blueprints;
pub(crate) mod prefix_check_if_rollback_needed;
pub(crate) mod prefix_first;
pub(crate) mod prefix_in_rollback;
pub(crate) mod prefix_start_rollback;

/// footgun protection - in case your clock ticking fn isn't running properly, this avoids
/// timewarp rolling back if the clock won't advance, since that would be an infinite loop.
pub(crate) fn sanity_check(
    game_clock: Res<GameClock>,
    opt_rb: Option<Res<Rollback>>,
    mut prev_frame: Local<u32>,
) {
    if **game_clock == 0 && opt_rb.is_some() {
        panic!(
            "⛔️ GameClock is on 0, but timewarp wants to rollback. {game_clock:?} rb:{:?}",
            opt_rb.unwrap().clone()
        );
    }
    if let Some(rb) = opt_rb {
        if *prev_frame == **game_clock
            && (rb.range.start == *prev_frame && rb.range.end != *prev_frame)
        {
            panic!(
            "⛔️ GameClock not advancing properly, and timewarp wants to rollback. {game_clock:?} rb:{rb:?}"
            );
        }
    }
    *prev_frame = **game_clock;
}
