use crate::*;
use bevy::prelude::*;
use bevy::reflect::Reflect;
use core::fmt::Debug;
use itertools::Itertools;
use std::fmt;

// From: https://github.com/jaynus/reliable.io/blob/master/rust/src/sequence_buffer.rs
// 3-clause bsd
// Copyright © 2017, The Network Protocol Company, Inc.

// need to augment for the server snapshot history component - it will be gappy.
// no value for every frame, lots of Nones. need to be able to seek fwd/back to find the nearest
// Some entry. might need to interp between?

#[derive(Resource, Clone, Reflect)]
pub struct SequenceBuffer<T>
where
    T: std::clone::Clone + Send + Sync + std::fmt::Debug,
{
    entries: Vec<Option<T>>,
    entry_sequences: Vec<u32>,
    sequence: FrameNumber,
}

impl<T> Debug for SequenceBuffer<T>
where
    T: Clone + Send + Sync + std::fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "SequenceBuffer[seq:{:?}, entry_seq:{:?}, entries:{:?}]",
            self.sequence,
            self.entry_sequences.iter().join(","),
            self.entries
        )
    }
}

impl<T> SequenceBuffer<T>
where
    T: std::clone::Clone + Send + Sync + std::fmt::Debug,
{
    pub fn with_capacity(size: usize) -> Self {
        let mut entries = Vec::with_capacity(size);
        let mut entry_sequences = Vec::with_capacity(size);

        entries.resize(size, None);
        entry_sequences.resize(size, 0xFFFF_FFFF);

        Self {
            sequence: 0,
            // newest_seq: 0,
            entries,
            entry_sequences,
        }
    }

    pub fn get(&self, sequence: FrameNumber) -> Option<&T> {
        let index = self.index(sequence);
        if self.entry_sequences[index] != sequence {
            return None;
        }
        self.entries[index].as_ref()
    }
    // pub fn get_mut(&mut self, sequence: FrameNumber) -> Option<&mut T> {
    //     let index = self.index(sequence);

    //     if self.entry_sequences[index] != u32::from(sequence) {
    //         return None;
    //     }
    //     self.entries[index].as_mut()
    // }

    // #[cfg_attr(feature = "cargo-clippy", allow(clippy::cast_possible_truncation))]
    pub fn insert(&mut self, data: T, sequence: FrameNumber) -> Result<(), TimewarpError> {
        if self.len() <= self.sequence as usize
            && sequence < (self.sequence - self.len() as FrameNumber)
        {
            return Err(TimewarpError::SequenceBufferFull);
        }
        if (sequence + 1) > self.sequence {
            self.remove_range(self.sequence..sequence);

            self.sequence = sequence + 1;
        }

        let index = self.index(sequence);

        self.entries[index] = Some(data);
        self.entry_sequences[index] = sequence;

        self.sequence = sequence + 1;

        Ok(())
    }

    // TODO: THIS IS INCLUSIVE END
    pub fn remove_range(&mut self, range: std::ops::Range<FrameNumber>) {
        for i in range.clone() {
            self.remove(i);
        }
        self.remove(range.end);
    }

    pub fn remove(&mut self, sequence: FrameNumber) {
        // TODO: validity check
        let index = self.index(sequence);
        self.entries[index] = None;
        self.entry_sequences[index] = 0xFFFF_FFFF;
    }

    pub fn reset(&mut self) {
        self.sequence = 0;
        // self.newest_seq = 0;
        for e in &mut self.entry_sequences {
            *e = 0;
        }
    }

    pub fn sequence(&self) -> FrameNumber {
        self.sequence
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() > 0
    }

    pub fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    #[inline]
    fn index(&self, sequence: FrameNumber) -> usize {
        (sequence % self.entries.len() as FrameNumber) as usize
    }
}
