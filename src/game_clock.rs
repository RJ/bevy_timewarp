use super::*;
use bevy::prelude::*;
use std::fmt;
use std::ops::Deref;

#[derive(Resource, Default)]
pub struct GameClock {
    pub frames_ahead: i8,
    frame: FrameNumber,
}

impl GameClock {
    pub fn new() -> Self {
        Self {
            frames_ahead: 0,
            frame: 0,
        }
    }
    // Gets current FrameNumber
    pub fn frame(&self) -> FrameNumber {
        self.frame
    }
    pub fn advance(&mut self, ticks: FrameNumber) {
        self.frame += ticks;
    }
    pub fn set(&mut self, frame: FrameNumber) {
        self.frame = frame;
    }
}

impl Deref for GameClock {
    type Target = FrameNumber;
    fn deref(&self) -> &Self::Target {
        &self.frame
    }
}

impl fmt::Debug for GameClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[[f={}]]", self.frame)
    }
}

impl fmt::Display for GameClock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[f={}]", self.frame)
    }
}
