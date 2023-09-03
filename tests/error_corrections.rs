/*
    In what scenarios should a TimewarpCorrection be generated?

    1)  When a non-anachronous entity receives a ServerSnapshot in the past for
        a component registered with correction logging:

        trigger_rollback_when_snapshot_added will detect the SS change,
        set comp_hist.diff_at_frame



*/

use bevy::prelude::*;
use bevy_timewarp::prelude::*;

mod test_utils;
use test_utils::*;

fn inc_frame(mut game_clock: ResMut<GameClock>, rb: Option<Res<Rollback>>) {
    game_clock.advance(1);
    info!("FRAME --> {:?} rollback:{rb:?}", game_clock.frame());
}

fn take_damage(mut q: Query<(Entity, &mut Enemy, &EntName)>) {
    for (entity, mut enemy, name) in q.iter_mut() {
        enemy.health -= 1;
        info!("{entity:?} took 1 damage -> {enemy:?} {name:?}");
    }
}

fn log_all(game_clock: Res<GameClock>, q: Query<(Entity, &Enemy, &EntName)>) {
    for tuple in q.iter() {
        info!("f:{:?} {tuple:?}", game_clock.frame());
    }
}

#[test]
fn error_correction() {
    let mut app = setup_test_app();

    app.register_rollback_with_correction_logging::<Enemy>();

    app.add_systems(
        FixedUpdate,
        (inc_frame, take_damage, log_all)
            .chain()
            .in_set(TimewarpTestSets::GameLogic),
    );

    // doing initial spawning here instead of a system in Setup, so we can grab entity ids:
    let e1 = app
        .world
        .spawn((
            Enemy { health: 10 },
            EntName {
                name: "E1".to_owned(),
            },
        ))
        .id();

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        0
    );

    tick(&mut app); // frame 1
    tick(&mut app); // frame 2
    tick(&mut app); // frame 3
    tick(&mut app); // frame 4

    // we just simulated frame 4
    let gc = app.world.get_resource::<GameClock>().unwrap();
    assert_eq!(gc.frame(), 4);

    // by now, these should be current values
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 6);

    assert_eq!(app.comp_val_at::<Enemy>(e1, 4).unwrap().health, 6);

    // let's pretend during frame 5 we get a message from the server saying that on frame 2, E1
    // ate a powerup, changing his health to 100.
    // our app's netcode would insert the authoritative (slightly outdated) values into ServerSnapshots.
    // then, the trigger_rollback_when_snapshot_added system would detect that
    // a new snapshot is available for `Enemy`, and schedule a rollback alongside setting the
    // diff_at_frame flag for the current frame, so a TimewarpCorrection is generated.

    let mut ss_e1 = app.world.get_mut::<ServerSnapshot<Enemy>>(e1).unwrap();
    ss_e1.insert(2, Enemy { health: 100 });

    // this message will be processed in the next tick - frame 5.
    // prior to this there shouldn't be a TimewarpCorrection component,
    // but it should be added.
    assert!(app.world.get::<TimewarpCorrection<Enemy>>(e1).is_none());

    tick(&mut app); // frame 5, we expect a rollback

    assert!(app.world.get::<TimewarpCorrection<Enemy>>(e1).is_some());

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    assert_eq!(app.comp_val_at::<Enemy>(e1, 2).unwrap().health, 100);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 3).unwrap().health, 99);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 4).unwrap().health, 98);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 5).unwrap().health, 97);

    // resimulation should have brought us back to frame 5.
    let gc = app.world.get_resource::<GameClock>().unwrap();
    assert_eq!(gc.frame(), 5);

    // now the meat of this test - we check that the before/after component values are correct
    // either side of the rollback that just happened on tick 5
    //
    // NB we actually can't view the pre-rollback comp value @ 5 because within the tick the
    // rollback runs and replaces it before the end of the tick.
    //
    // we already asserted that at tick 4 E1's health was 6, so we'd expect it to be 5 at tick 5.
    let twc = app.world.get::<TimewarpCorrection<Enemy>>(e1).unwrap();
    // component values before/after the rollback
    assert_eq!(twc.before.health, 5);
    assert_eq!(twc.after.health, 97);
    assert_eq!(twc.frame, 5);

    warn!("{twc:?}");
    // NB rendering is happening in PostUpdate, which runs after FixedUpdate
    //    * FixedUpdate @ 4 (normal frame)
    //    * PostUpdate render
    //    * FixedUpdate @ 5 (applies rollback data, by end of this we've snapped stuff)
    //    * PostUpdate render
    //
    // so what error correction do we want to caputure?
    // we never actually rendered E1 at the locally simulated tick 5, since that value was
    // calculated but then replaced during rollback within the same frame.
    //
    // the locally simulated value of tick 5 is where the user would anticipate the entity to be,
    // even if we never rendered it at that position - ie a natural progression of the local simulation.
    //
    // Smoothing might work like this: 97 - 5 = 92.
    // Apply a visual diff of -92 to the component, and quickly blend it towards 0.
    // in other words, the Visual Diff = (before - after) value
    //
    // (obviously you wouldn't visually blend "health", just assume this is a position or something)

    // do another normal tick
    tick(&mut app); // frame 6

    // correction values shouldn't have changed – there was no rollback that frame
    let twc = app.world.get::<TimewarpCorrection<Enemy>>(e1).unwrap();
    assert_eq!(twc.before.health, 5);
    assert_eq!(twc.after.health, 97);
    assert_eq!(twc.frame, 5);

    tick(&mut app); // frame 7
    tick(&mut app); // frame 8
    tick(&mut app); // frame 9

    assert_eq!(app.comp_val_at::<Enemy>(e1, 7).unwrap().health, 95);

    assert_eq!(app.comp_val_at::<Enemy>(e1, 9).unwrap().health, 93);
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 93);

    // supply frame 7 value at known local value, ie server confirms our simulation value
    let mut ss_e1 = app.world.get_mut::<ServerSnapshot<Enemy>>(e1).unwrap();
    ss_e1.insert(7, Enemy { health: 95 });

    tick(&mut app); // frame 10 - rollback? no. should be bypassed because prediction was right

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    assert_eq!(app.comp_val_at::<Enemy>(e1, 10).unwrap().health, 92);
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 92);

    // no correction should be created since server confirmed predicted value,
    // thus the frame on the TimewarpCorrection should still be 5, from the earlier correction
    let twc = app.world.get::<TimewarpCorrection<Enemy>>(e1).unwrap();
    assert_eq!(twc.frame, 5);
}
