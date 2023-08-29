use crate::{FrameBuffer, FrameNumber};
use bevy::prelude::*;

/// entities with NotRollbackable are ignored, even if they have components which
/// have been registered for rollback.
#[derive(Component)]
pub struct NotRollbackable;

/// Tells us this entity is to be rendered in the past
/// based on snapshots received from the server.
/// aka Predicted maybe, if we actually predict inputs into the future?
#[derive(Component, Clone, Debug)]
pub struct Anachronous {
    pub frames_behind: FrameNumber,
    /// frame of last server snapshot update.
    /// used in calculation for frame lag for anachronous entities
    newest_snapshot_frame: FrameNumber,
    /// frame number of most recent input command for this entity
    /// used in calculation for frame lag for anachronous entities
    newest_input_frame: FrameNumber,
}
impl Anachronous {
    pub fn new(frames_behind: FrameNumber) -> Self {
        Self {
            frames_behind,
            newest_snapshot_frame: 0,
            newest_input_frame: 0,
        }
    }
    pub fn newest_snapshot_frame(&self) -> FrameNumber {
        self.newest_snapshot_frame
    }
    pub fn newest_input_frame(&self) -> FrameNumber {
        self.newest_input_frame
    }
    pub fn set_newest_snapshot_frame(&mut self, newest_frame: FrameNumber) -> bool {
        if newest_frame > self.newest_snapshot_frame {
            self.newest_snapshot_frame = newest_frame;
            true
        } else {
            false
        }
    }
    pub fn set_newest_input_frame(&mut self, newest_frame: FrameNumber) -> bool {
        if newest_frame > self.newest_input_frame {
            self.newest_input_frame = newest_frame;
            true
        } else {
            false
        }
    }
}

/// Used when you want to insert a component T, but for an older frame.
/// insert this to an entity for an older frame will trigger a rollback.
/// eg:
/// ```rust,ignore
/// commands.entity(e).insert(InsertComponentAtFrame::<Shield>(shield_comp, past_frame))
/// ```
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
        self.values.insert(frame, val);
    }
}

/// used to record component birth/death ranges in ComponentHistory.
/// (start, end) – can be open-ended if end is None.
pub type FrameRange = (FrameNumber, Option<FrameNumber>);

/// Buffers component values for the last few frames.
#[derive(Component)]
pub struct ComponentHistory<T: Component + Clone + std::fmt::Debug> {
    pub values: FrameBuffer<T>,        // not pub!
    pub alive_ranges: Vec<FrameRange>, // inclusive! unlike std:range
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
    // adding entity just for debugging print outs.
    pub fn insert(&mut self, frame: FrameNumber, val: T, entity: &Entity) {
        trace!("CH.Insert {entity:?} {frame} = {val:?}");
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