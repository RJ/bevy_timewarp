use crate::FrameNumber;
use bevy::prelude::*;
use std::{ops::Range, time::Duration};

#[derive(Resource, Debug, Copy, Clone, Default)]
pub struct TimewarpConfig {
    /// how many frames of old component values should we buffer?
    /// can't roll back any further than this. will depend on network lag and game mechanics.
    pub rollback_window: FrameNumber,
    /// if set to true, a rollback will be initiated even if
    /// the stored predicted value matches the server snapshot.
    /// meant as a worst-case scenario for checking performance really.
    pub force_rollback_always: bool,
}

/// Updated whenever we perform a rollback
#[derive(Resource, Debug, Default)]
pub struct RollbackStats {
    pub num_rollbacks: u64,
}

/// If this resource exists, we are doing a rollback. Insert it to initate one manually.
/// Normally you would never manually insert a Rollback, it would be trigger automatically
/// in one of the following ways:
///
/// * You insert a `InsertComponentAtFrame<T>` for a past frame
/// * You supply ServerSnapshot<T> data for a past frame
///
#[derive(Resource, Debug, Clone)]
pub struct Rollback {
    /// the range of frames, start being the target we rollback to
    pub range: Range<FrameNumber>,
    /// we preserve the original FixedUpdate period here and restore after rollback completes.
    /// (during rollback, we set the FixedUpdate period to 0.0, to effect fast-forward resimulation)
    pub original_period: Option<Duration>,
}
impl Rollback {
    /// The range start..end contains all values with start <= x < end. It is empty if start >= end.
    pub fn new(start: FrameNumber, end: FrameNumber) -> Self {
        Self {
            range: Range { start, end },
            original_period: None,
        }
    }
}

/// Every time a rollback completes, before the `Rollback` resources is removed,
/// we copy it into the `PreviousRollback` resources.
///
/// This is mainly so integration tests can tell wtaf is going on :)
#[derive(Resource, Debug)]
pub struct PreviousRollback(pub Rollback);

/// Add to entity to despawn cleanly in the rollback world
#[derive(Default, Component, Debug, Clone, Copy, PartialEq)]
pub struct DespawnMarker(pub Option<FrameNumber>);

impl DespawnMarker {
    pub fn new() -> Self {
        Self(None)
    }
    pub fn for_frame(frame: FrameNumber) -> Self {
        Self(Some(frame))
    }
}
