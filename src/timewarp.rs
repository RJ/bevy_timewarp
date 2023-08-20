use crate::systems::*;
use bevy::prelude::*;

use super::*;
use std::{ops::Range, time::Duration};

#[derive(Component)]
pub struct NotRollbackable;

/// the marker component that tells us this entity is to be rendered in the past
/// based on snapshots received from the server.
#[derive(Component, Clone, Debug)]
pub struct Anachronous {
    pub frames_behind: FrameNumber,
}
impl Anachronous {
    pub fn new(frames_behind: FrameNumber) -> Self {
        Self { frames_behind }
    }
}

#[derive(Resource, Debug, Default)]
pub struct RollbackStats {
    pub num_rollbacks: u64,
}

/// if this resource exists, we are doing a rollback. Insert it to initate one.
#[derive(Resource, Debug)]
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
    pub fn ensure_start_covers(&mut self, start: FrameNumber) {
        if start < self.range.start {
            self.range.start = start;
        }
    }
}

/// used to record component birth/death ranges in ComponentHistory.
/// (start, end) – can be open-ended if end is None.
pub type FrameRange = (FrameNumber, Option<FrameNumber>);

/// Used when you want to insert a component T, but for an older frame.
/// insert this to an entity for an older frame will trigger a rollback.
#[derive(Component, Debug)]
pub struct InsertComponentAtFrame<T: Component + Clone + std::fmt::Debug> {
    pub component: T,
    pub frame: FrameNumber,
}
impl<T: Component + Clone + std::fmt::Debug> InsertComponentAtFrame<T> {
    pub fn new(frame: FrameNumber, component: T) -> Self {
        Self { component, frame }
    }
}

/// entities with components that were registered with error correction logging will receive
/// one of these components, updated with before/after values when a simulation correction
/// resulting from a rollback and resimulate causes a snap.
/// ie, the values before and after the rollback differ.
/// in your game, look for Changed<TimewarpCorrection<T>> and use for any visual smoothing/interp stuff.
#[derive(Component, Debug, Clone)]
pub struct TimewarpCorrection<T: Component + Clone + std::fmt::Debug> {
    pub before: T,
    pub after: T,
    pub frame: FrameNumber,
}

/// Buffers the last few authoritative component values received from the server
#[derive(Component)]
pub struct ServerSnapshot<T: Component + Clone + std::fmt::Debug> {
    pub values: FrameBuffer<T>,
}
impl<T: Component + Clone + std::fmt::Debug> ServerSnapshot<T> {
    pub fn with_capacity(len: usize) -> Self {
        Self {
            values: FrameBuffer::with_capacity(len),
        }
    }
    pub fn at_frame(&self, frame: FrameNumber) -> Option<&T> {
        self.values.get(frame)
    }
    pub fn insert(&mut self, frame: FrameNumber, val: T) {
        // TODO this should never be allowed to fail?
        self.values.insert(frame, val);
    }
}

/// Buffers component values for the last few frames.
#[derive(Component)]
pub struct ComponentHistory<T: Component + Clone + std::fmt::Debug> {
    pub values: FrameBuffer<T>,        // not pub!
    pub alive_ranges: Vec<FrameRange>, // inclusive! unlike std:range
    pub most_recent_authoritative_frame: FrameNumber,
    /// when we insert at this frame, compute diff between newly inserted val and whatever already exists in the buffer.
    /// this is for visual smoothing post-rollback.
    /// (if the simulation is perfect the diff would be zero, but will be unavoidably non-zero when dealing with collisions between anachronous entities for example.)
    pub diff_at_frame: Option<FrameNumber>,
    pub correction_logging_enabled: bool,
}

// lazy first version - don't need a clone each frame if value hasn't changed!
// just store once and reference from each unchanged frame number.
impl<T: Component + Clone + std::fmt::Debug> ComponentHistory<T> {
    pub fn with_capacity(len: usize, birth_frame: FrameNumber) -> Self {
        let mut this = Self {
            values: FrameBuffer::with_capacity(len),
            alive_ranges: Vec::new(),
            most_recent_authoritative_frame: birth_frame,
            diff_at_frame: None,
            correction_logging_enabled: false,
        };
        this.report_birth_at_frame(birth_frame);
        this
    }
    /// will compute and insert `TimewarpCorrection`s when snapping
    pub fn enable_correction_logging(&mut self) {
        self.correction_logging_enabled = true;
    }
    pub fn at_frame(&self, frame: FrameNumber) -> Option<&T> {
        self.values.get(frame)
    }
    pub fn insert_authoritative(&mut self, frame: FrameNumber, val: T) {
        self.most_recent_authoritative_frame = frame;
        self.insert(frame, val);
    }
    pub fn insert(&mut self, frame: FrameNumber, val: T) {
        self.values.insert(frame, val);
    }
    /// removes values buffered for this frame, and greater frames.
    pub fn remove_frame_and_beyond(&mut self, frame: FrameNumber) {
        self.values
            .remove_entries_newer_than(frame.saturating_sub(1));
    }
    pub fn alive_at_frame(&self, frame: FrameNumber) -> bool {
        for (start, maybe_end) in &self.alive_ranges {
            if *start <= frame && (maybe_end.is_none() || maybe_end.unwrap() > frame) {
                return true;
            }
        }
        false
    }
    pub fn report_birth_at_frame(&mut self, frame: FrameNumber) {
        debug!("component birth @ {frame} {:?}", std::any::type_name::<T>());
        self.alive_ranges.push((frame, None));
    }
    pub fn report_death_at_frame(&mut self, frame: FrameNumber) {
        debug!("component death @ {frame} {:?}", std::any::type_name::<T>());
        self.alive_ranges.last_mut().unwrap().1 = Some(frame);
    }
}

#[derive(PartialEq, Eq)]
pub enum ComponentRollbackOptions {
    Nothing = 0,
    CorrectionLogging = 1,
}

/// trait for registering components with the rollback system.
pub trait TimewarpTraits {
    /// register component for rollback
    fn register_rollback<T: Component + Clone + std::fmt::Debug>(&mut self) -> &mut Self;
    /// register component for rollback, and also update a TimewarpCorrection component when snapping
    fn register_rollback_with_correction_logging<T: Component + Clone + std::fmt::Debug>(
        &mut self,
    ) -> &mut Self;
    /// register component for rollback with additional options
    fn register_rollback_with_options<T: Component + Clone + std::fmt::Debug>(
        &mut self,
        opts: ComponentRollbackOptions,
    ) -> &mut Self;
}

impl TimewarpTraits for App {
    fn register_rollback<T: Component + Clone + std::fmt::Debug>(&mut self) -> &mut Self {
        self.register_rollback_with_options::<T>(ComponentRollbackOptions::Nothing)
    }
    fn register_rollback_with_correction_logging<T: Component + Clone + std::fmt::Debug>(
        &mut self,
    ) -> &mut Self {
        self.register_rollback_with_options::<T>(ComponentRollbackOptions::CorrectionLogging)
    }
    fn register_rollback_with_options<T: Component + Clone + std::fmt::Debug>(
        &mut self,
        opts: ComponentRollbackOptions,
    ) -> &mut Self {
        // we need to insert the ComponentTimeline<T> component to log history
        // and add systems to update/apply it
        if opts == ComponentRollbackOptions::CorrectionLogging {
            self.add_systems(
                FixedUpdate,
                (
                    add_timewarp_buffer_components::<T, true>
                        .in_set(TimewarpSet::RollbackPreUpdate),
                )
                    .chain(),
            );
        } else {
            self.add_systems(
                FixedUpdate,
                (
                    add_timewarp_buffer_components::<T, false>
                        .in_set(TimewarpSet::RollbackPreUpdate),
                )
                    .chain(),
            );
        }
        self
            // we want to record frame values even if we're about to rollback -
            // we need values pre-rb to diff against post-rb versions.
            .add_systems(
                FixedUpdate,
                record_component_history_values::<T>
                    .before(check_for_rollback_completion)
                    .in_set(TimewarpSet::RollbackPreUpdate),
            )
            .add_systems(
                FixedUpdate,
                // this may end up inserting a rollback resource
                (
                    insert_components_at_prior_frames::<T>,
                    (
                        apply_snapshots_and_rollback_for_non_anachronous::<T>,
                        apply_snapshots_and_snap_for_anachronous::<T>,
                    ),
                )
                    .chain()
                    .after(check_for_rollback_completion)
                    .run_if(not(resource_added::<Rollback>()))
                    .in_set(TimewarpSet::RollbackPreUpdate),
            )
            // NB after rollback_initiated, the game_clock will already be rolled back to prior frame
            .add_systems(
                FixedUpdate,
                rollback_initiated_for_component::<T>
                    .after(rollback_initiated)
                    .run_if(resource_added::<Rollback>())
                    .in_set(TimewarpSet::RollbackInitiated),
            )
            .add_systems(
                FixedUpdate,
                (
                    (
                        reremove_components_inserted_during_rollback_at_correct_frame::<T>,
                        reinsert_components_removed_during_rollback_at_correct_frame::<T>,
                    )
                        .run_if(resource_exists::<Rollback>()),
                    (
                        // this grants newly added despawnmarkers the current frame -
                        // a convenience, since they are typically inserted with None via DespawnMarker::new()
                        process_freshly_added_despawn_markers::<T>,
                        // which is needed before this system runs to use them:
                        do_actual_despawn_after_rollback_frames_from_despawn_marker,
                    )
                        .chain()
                        .run_if(not(resource_exists::<Rollback>())),
                )
                    .in_set(TimewarpSet::RollbackPreUpdate)
                    .before(check_for_rollback_completion),
            )
            // don't log component added/removed if we are in a rollback frame
            .add_systems(
                FixedUpdate,
                (
                    // Recording component births. this does the Added<> query, and bails if in rollback
                    // so that the Added query is refreshed.
                    record_component_added_to_alive_ranges::<T>,
                    // removed components gets cleared during rollback.
                    record_component_removed_to_alive_ranges::<T>
                        .run_if(not(resource_exists::<Rollback>())),
                    // During rollback, we want to actively wipe the RemovedComponents queue
                    // since not relevant. Otherwise we end up processing all the removed components
                    // on the first non-rollback frame, for components that were removed and reinserted
                    // as part of the rollback process, if the rollback frame overlaps a component
                    // birth/death boundary - which is fairly common.
                    // (RemovedComponents<T> is basically like an Events<>)
                    clear_removed_components_queue::<T>.run_if(resource_exists::<Rollback>()),
                )
                    .chain()
                    .before(check_for_rollback_completion)
                    .in_set(TimewarpSet::RollbackPreUpdate),
            )
    }
}
