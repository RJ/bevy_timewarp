use crate::FrameNumber;
use bevy::{
    ecs::schedule::{InternedScheduleLabel, ScheduleLabel},
    prelude::*,
    utils::intern::Interned,
};
use std::{ops::Range, time::Duration};

/// if various systems request rollbacks to different frames within one tick, when consolidating
/// those requests into an actionable Rollback, do we choose the oldest or newest frame from the
/// list of requests?
#[derive(Debug, Copy, Clone)]
pub enum RollbackConsolidationStrategy {
    Oldest,
    Newest,
}

#[derive(Resource, Debug, Clone)]
pub struct TimewarpConfig {
    /// if you can update some entities one frame and some another, ie you don't receive
    /// entire-world update, set this to Oldest, or you will miss data.
    /// the default is Newest (for replicon, which is entire-world updates only atm)
    pub consolidation_strategy: RollbackConsolidationStrategy,
    /// how many frames of old component values should we buffer?
    /// can't roll back any further than this. will depend on network lag and game mechanics.
    pub rollback_window: FrameNumber,
    /// if set to true, a rollback will be initiated even if
    /// the stored predicted value matches the server snapshot.
    /// meant as a worst-case scenario for checking performance really.
    pub force_rollback_always: bool,
    /// schedule in which our `after_set` and rollback systems run, defaults to FixedUpdate
    pub schedule: InternedScheduleLabel,
    /// first set containing game logic
    pub first_set: Interned<dyn SystemSet>,
    /// last set containing game logic
    pub last_set: Interned<dyn SystemSet>,
}

impl TimewarpConfig {
    /// Makes a new timewarp config, with defaults:
    /// rollback_window: 30
    /// forced_rollback: false
    /// schedule: FixedUpdate
    pub fn new(first_set: impl SystemSet, last_set: impl SystemSet) -> Self {
        Self {
            consolidation_strategy: RollbackConsolidationStrategy::Newest,
            first_set: first_set.intern(),
            last_set: last_set.intern(),
            // and defaults, override with builder fns:
            rollback_window: 30,
            force_rollback_always: false,
            schedule: FixedUpdate.intern(),
        }
    }
    pub fn with_schedule(mut self, schedule: impl ScheduleLabel) -> Self {
        self.schedule = schedule.intern();
        self
    }
    pub fn with_forced_rollback(mut self, enabled: bool) -> Self {
        self.force_rollback_always = enabled;
        self
    }
    pub fn with_rollback_window(mut self, num_frames: FrameNumber) -> Self {
        self.rollback_window = num_frames;
        self
    }
    pub fn with_consolidation_strategy(mut self, strategy: RollbackConsolidationStrategy) -> Self {
        self.consolidation_strategy = strategy;
        self
    }

    pub fn first_set(&self) -> Interned<dyn SystemSet> {
        self.first_set
    }
    pub fn last_set(&self) -> Interned<dyn SystemSet> {
        self.last_set
    }
    pub fn forced_rollback(&self) -> bool {
        self.force_rollback_always
    }
    pub fn schedule(&self) -> Interned<dyn ScheduleLabel> {
        self.schedule
    }
    pub fn rollback_window(&self) -> FrameNumber {
        self.rollback_window
    }
    pub fn consolidation_strategy(&self) -> RollbackConsolidationStrategy {
        self.consolidation_strategy
    }
    pub fn set_consolidation_strategy(&mut self, strategy: RollbackConsolidationStrategy) {
        self.consolidation_strategy = strategy;
    }
    pub fn is_within_rollback_window(
        &self,
        current_frame: FrameNumber,
        target_frame: FrameNumber,
    ) -> bool {
        target_frame > current_frame - self.rollback_window
    }
}

/// Updated whenever we perform a rollback
#[derive(Resource, Debug, Default)]
pub struct RollbackStats {
    pub num_rollbacks: u64,
    pub range_faults: u64,
    pub non_rollback_updates: u64,
}

/// If this resource exists, we are doing a rollback. Insert it to initate one manually.
/// Normally you would never manually insert a Rollback, it would be trigger automatically
/// in one of the following ways:
///
/// * You insert a `InsertComponentAtFrame<T>` for a past frame
/// * You insert a `AssembleBlueprintAtFrame<T>` for a past frame
/// * You supply ServerSnapshot<T> data for a past frame
///
#[derive(Resource, Debug, Clone)]
pub struct Rollback {
    /// the range of frames, start being the target we resimulate first
    pub range: Range<FrameNumber>,
    /// we preserve the original FixedUpdate period here and restore after rollback completes.
    /// (during rollback, we set the FixedUpdate period to 0.0, to effect fast-forward resimulation)
    pub original_period: Option<Duration>,
}
impl Rollback {
    /// `end` is the last frame to be resimulated
    pub fn new(
        first_frame_to_resimulate: FrameNumber,
        last_frame_to_resimulate: FrameNumber,
    ) -> Self {
        Self {
            range: Range {
                start: first_frame_to_resimulate,
                end: last_frame_to_resimulate,
            },
            original_period: None,
        }
    }
}

/// systems that want to initiate a rollback write one of these to
/// the Events<RollbackRequest> queue.
#[derive(Event, Debug)]
pub struct RollbackRequest(FrameNumber);

impl RollbackRequest {
    pub fn resimulate_this_frame_onwards(frame: FrameNumber) -> Self {
        if frame == 0 {
            warn!("RollbackRequest(0)!");
        }
        Self(frame)
    }
    pub fn frame(&self) -> FrameNumber {
        self.0
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
