use super::*;
use bevy::prelude::*;

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
