use bevy::prelude::*;
use bevy_timewarp::prelude::*;

// doesn't really matter what this is, since we simulate the time passing for testing.
// however if it's low, say 16ms, and the test takes a while to execute, you could end up running
// more ticks than you want. setting it to a high value avoids this.
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
    let tw_config = TimewarpConfig::new(TimewarpTestSets::GameLogic, TimewarpTestSets::GameLogic)
        .with_rollback_window(TEST_ROLLBACK_WINDOW)
        .with_schedule(FixedUpdate);
    let mut app = App::new();
    app.add_plugins(bevy::log::LogPlugin {
        level: bevy::log::Level::TRACE,
        filter: "bevy_timewarp=trace".to_string(),
    });
    app.add_plugins(TimewarpPlugin::new(tw_config));
    app.add_plugins(bevy::time::TimePlugin::default());
    app.insert_resource(FixedTime::new(TIMESTEP));
    warn!("⏱️Instant::now= {:?}", bevy::utils::Instant::now());
    app
}

// Simulate that our fixed timestep has elapsed
// and do 1 app.update
pub fn tick(app: &mut App) {
    let mut fxt = app.world.resource_mut::<FixedTime>();
    let period = fxt.period;
    fxt.tick(period);
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
