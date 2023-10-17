use crate::FrameNumber;
use bevy::{
    ecs::schedule::{BoxedSystemSet, ScheduleLabel},
    prelude::*,
};
use std::{ops::Range, time::Duration};

#[derive(Resource, Debug, Clone)]
pub struct TimewarpConfig {
    /// how many frames of old component values should we buffer?
    /// can't roll back any further than this. will depend on network lag and game mechanics.
    pub rollback_window: FrameNumber,
    /// if set to true, a rollback will be initiated even if
    /// the stored predicted value matches the server snapshot.
    /// meant as a worst-case scenario for checking performance really.
    pub force_rollback_always: bool,
    /// schedule in which our `after_set` and rollback systems run, defaults to FixedUpdate
    pub schedule: Box<dyn ScheduleLabel>,
    /// first set containing game logic
    pub first_set: BoxedSystemSet,
    /// last set containing game logic
    pub last_set: BoxedSystemSet,
}

impl TimewarpConfig {
    /// Makes a new timewarp config, with defaults:
    /// rollback_window: 30
    /// forced_rollback: false
    /// schedule: FixedUpdate
    pub fn new(first_set: impl SystemSet, last_set: impl SystemSet) -> Self {
        Self {
            first_set: Box::new(first_set),
            last_set: Box::new(last_set),
            // and defaults, override with builder fns:
            rollback_window: 30,
            force_rollback_always: false,
            schedule: Box::new(FixedUpdate),
        }
    }
    pub fn set_schedule(mut self, schedule: impl ScheduleLabel) -> Self {
        self.schedule = Box::new(schedule);
        self
    }
    pub fn set_forced_rollback(mut self, enabled: bool) -> Self {
        self.force_rollback_always = enabled;
        self
    }
    pub fn set_rollback_window(mut self, num_frames: FrameNumber) -> Self {
        self.rollback_window = num_frames;
        self
    }
    pub fn first_set(&self) -> BoxedSystemSet {
        self.first_set.as_ref().dyn_clone()
    }
    pub fn last_set(&self) -> BoxedSystemSet {
        self.last_set.as_ref().dyn_clone()
    }
    pub fn forced_rollback(&self) -> bool {
        self.force_rollback_always
    }
    pub fn schedule(&self) -> Box<dyn ScheduleLabel> {
        self.schedule.dyn_clone()
    }
    pub fn rollback_window(&self) -> FrameNumber {
        self.rollback_window
    }
}

/// Updated whenever we perform a rollback
#[derive(Resource, Debug, Default)]
pub struct RollbackStats {
    pub num_rollbacks: u64,
    pub range_faults: u64,
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
    aborted: bool,
}
impl Rollback {
    /// The range start..end contains all values with start <= x < end. It is empty if start >= end.
    pub fn new(start: FrameNumber, end: FrameNumber) -> Self {
        Self {
            range: Range { start, end },
            original_period: None,
            aborted: false,
        }
    }
    pub fn abort(&mut self) {
        self.aborted = true;
    }
    pub fn aborted(&self) -> bool {
        self.aborted
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
