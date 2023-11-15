use std::time::Duration;

use bevy::prelude::*;
use bevy_timewarp::prelude::*;

/// Is arbitrarily large amount of time, such that no automatically run `FixedUpdate` schedules occur
pub const TIMESTEP: std::time::Duration = std::time::Duration::from_millis(100000);
pub const TEST_ROLLBACK_WINDOW: FrameNumber = 10;

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimewarpTestSets {
    GameLogic, // game logic here
}

#[derive(Component, Default, Debug, Clone, PartialEq)]
pub struct Enemy {
    pub health: i32,
}
#[derive(Component, Default, Debug, Clone)]
pub struct EntName {
    pub name: String,
}

pub fn setup_test_app() -> App {
    let mut app = App::new();

    let tw_config = TimewarpConfig::new(TimewarpTestSets::GameLogic, TimewarpTestSets::GameLogic)
        .with_rollback_window(TEST_ROLLBACK_WINDOW)
        .with_schedule(FixedUpdate);

    app.add_plugins(bevy::log::LogPlugin {
        level: bevy::log::Level::TRACE,
        filter: "bevy_timewarp=trace".to_string(),
    });
    app.add_plugins(TimewarpPlugin::new(tw_config));
    app.add_plugins(bevy::time::TimePlugin);

    // This ensures that the `FixedUpdate` schedule is run exactly once per frame
    // by making Time<Virtual> (which is what determines when `FixedTime` is automatically run)
    // increment *really slowly* and the `FixedUpdate` schedule run after a *very long*
    // amount of time.
    app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
        // Should be really small compared to [TIMESTEP]
        Duration::from_nanos(1),
    ));
    app.add_systems(bevy::app::RunFixedUpdateLoop, |world: &mut World| {
        // Manually runs the `FixedUpdate` schedule every `Update` cycle
        world.run_schedule(FixedUpdate);
    });
    app.insert_resource(Time::<Fixed>::from_duration(TIMESTEP));

    warn!("⏱️Instant::now= {:?}", bevy::utils::Instant::now());
    app
}

// Simulate that our fixed timestep has elapsed
// and do 1 app.update
pub fn tick(app: &mut App) {
    app.update();
    let f = app.world.resource::<GameClock>().frame();
    info!("end of update for {f} ----------------------------------------------------------");
}

// some syntactic sugar, just to make tests less of an eyesore:
pub(crate) trait TimewarpTestTraits {
    fn comp_val_at<T: TimewarpComponent>(&self, entity: Entity, frame: FrameNumber) -> Option<&T>;
}

impl TimewarpTestTraits for App {
    /// "Give me an Option<T> for the value of the Component T beloning to this entity, at a specific frame"
    fn comp_val_at<T: TimewarpComponent>(&self, entity: Entity, frame: FrameNumber) -> Option<&T> {
        self.world
            .get::<ComponentHistory<T>>(entity)
            .expect("Should be a ComponentHistory here")
            .values
            .get(frame)
    }
}
