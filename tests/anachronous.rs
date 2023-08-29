/// test an Anachronous entity - one that is in the past, relative to ourselves.
/// used for other players. we show their exact movements but delayed just enough so there's time
/// for the server to send us their inputs for the frames we need to simulate.
/// Typically Anachronous{frames_behind} value is tuned at runtime based on network stats.
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

/// an update arrives just for our anachronous entity
#[test]
fn anachronous_ss_triggers_rollback() {
    let mut app = setup_test_app();

    app.register_rollback::<Enemy>();

    app.add_systems(
        FixedUpdate,
        (inc_frame, take_damage, log_all)
            .chain()
            .in_set(TimewarpTestSets::GameLogic),
    );

    let e1 = app
        .world
        .spawn((
            Enemy { health: 10 },
            EntName {
                name: "E1".to_owned(),
            },
            Anachronous::new(3), // this entity is delayed by 3 frames
        ))
        .id();
    tick(&mut app); // frame 1
    tick(&mut app); // frame 2
    tick(&mut app); // frame 3
    tick(&mut app); // frame 4
    tick(&mut app); // frame 5
    tick(&mut app); // frame 6

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        0
    );

    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 4);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 6).unwrap().health, 4);

    app.world
        .get_mut::<ServerSnapshot<Enemy>>(e1)
        .unwrap()
        .insert(2, Enemy { health: 100 });

    // this should trigger a rollback to frame 2 + frames_behind = 5
    // at which point, because E1 is anachronous, it should snap in the value at frame
    // 5 - frames_behind = 2, which is the authoritative snapshot we just inserted.
    // then resim forward.. (in game, would be applying stored inputs too)

    tick(&mut app); // frame 7, RB

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    let prb = app
        .world
        .get_resource::<PreviousRollback>()
        .expect("Should be a PreviousRollback ");
    assert_eq!(prb.0.range.start, 5);
    assert_eq!(prb.0.range.end, 7);

    // frames_behind is 3, so at frame 5 we load data for frame 2 (ie, the SS, health = 100)
    // so f5 = 100, f6 = 99, f7 = 98
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 98);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 7).unwrap().health, 98);
}

/// an update arrives for our anachronous entity and a non-anchronous one in same frame.
/// this causes rollback to go more into the past than the SS frame for anach.
#[test]
fn anachronous_and_non_anachronous_ss_triggers_rollback() {
    let mut app = setup_test_app();

    app.register_rollback::<Enemy>();

    app.add_systems(
        FixedUpdate,
        (inc_frame, take_damage, log_all)
            .chain()
            .in_set(TimewarpTestSets::GameLogic),
    );

    // an anachronous entity
    let e1 = app
        .world
        .spawn((
            Enemy { health: 10 },
            EntName {
                name: "E1".to_owned(),
            },
            Anachronous::new(3), // this entity is delayed by 3 frames
        ))
        .id();

    // a contemporary entity
    let e2 = app
        .world
        .spawn((
            Enemy { health: 100 },
            EntName {
                name: "E2".to_owned(),
            },
        ))
        .id();

    tick(&mut app); // frame 1
    tick(&mut app); // frame 2
    tick(&mut app); // frame 3
    tick(&mut app); // frame 4
    tick(&mut app); // frame 5
    tick(&mut app); // frame 6

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        0
    );

    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 4);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 6).unwrap().health, 4);

    // SS is going to arrive next tick, ie frame 7.
    // it will contain a state update for our anach entity, and for our contemporary.

    // supply update to anachronous entity
    //
    // this should trigger a rollback to frame 5 (ie, 2 + frames_behind )
    // at which point, because E1 is anachronous, it should snap in the value at frame
    // 5 - frames_behind = 2, which is the authoritative snapshot we just inserted.
    // then resim forward.. (in game, would be applying stored inputs too)
    app.world
        .get_mut::<ServerSnapshot<Enemy>>(e1)
        .unwrap()
        .insert(2, Enemy { health: 100 });

    // supply update to contemporary entity
    //
    // this will trigger a rollback to frame 2
    app.world
        .get_mut::<ServerSnapshot<Enemy>>(e2)
        .unwrap()
        .insert(2, Enemy { health: 1000 });

    // rollbacks from both events will be consolidated resulting in a rollback to frame 2.

    tick(&mut app); // frame 7, RB

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    let prb = app
        .world
        .get_resource::<PreviousRollback>()
        .expect("Should be a PreviousRollback");
    assert_eq!(prb.0.range.start, 2);
    assert_eq!(prb.0.range.end, 7);

    // frames_behind is 3, so at frame 5 we load data for frame 2 (ie, the SS, health = 100)
    // so f5 = 100, f6 = 99, f7 = 98
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 98);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 7).unwrap().health, 98);
}

#[test]
fn anachronous() {
    let mut app = setup_test_app();

    app.register_rollback::<Enemy>();

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

    let e2 = app
        .world
        .spawn((
            Enemy { health: 10 },
            EntName {
                name: "E2".to_owned(),
            },
            Anachronous::new(4),
        ))
        .id();

    tick(&mut app); // frame 1
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 9);
    // before any ServerSnapshots we haven't rolled back..
    assert_eq!(app.world.get::<Enemy>(e2).unwrap().health, 9);

    tick(&mut app); // frame 2
    tick(&mut app); // frame 3
    tick(&mut app); // frame 4
    tick(&mut app); // frame 5

    // we just simulated frame 5
    assert_eq!(app.world.get_resource::<GameClock>().unwrap().frame(), 5);

    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 5);
    assert_eq!(app.world.get::<Enemy>(e2).unwrap().health, 5);

    assert_eq!(app.comp_val_at::<Enemy>(e1, 3).unwrap().health, 7);
    assert_eq!(app.comp_val_at::<Enemy>(e2, 3).unwrap().health, 7);

    assert_eq!(app.comp_val_at::<Enemy>(e1, 5).unwrap().health, 5);
    assert_eq!(app.comp_val_at::<Enemy>(e2, 5).unwrap().health, 5);

    // this would trigger a rollback to frame 4 next tick:
    app.world
        .get_mut::<ServerSnapshot<Enemy>>(e1)
        .unwrap()
        .insert(3, Enemy { health: 100 });
    // this entity is anachronous, so won't be changed by the forthcoming rollback
    app.world
        .get_mut::<ServerSnapshot<Enemy>>(e2)
        .unwrap()
        .insert(3, Enemy { health: 1000 });

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        0
    );

    tick(&mut app); // frame 6
    assert_eq!(app.world.get_resource::<GameClock>().unwrap().frame(), 6);
    // expecting 1 rollback because non-anachronous E1 will apply the new snapshot value and rollback
    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );
    assert_eq!(app.comp_val_at::<Enemy>(e1, 3).unwrap().health, 100);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 6).unwrap().health, 97);

    // E2 shouldn't have changed in rollback
    assert_eq!(app.comp_val_at::<Enemy>(e2, 3).unwrap().health, 7);
    assert_eq!(app.comp_val_at::<Enemy>(e2, 6).unwrap().health, 4);

    // next tick is 7.
    // timewarp should notice that we have a serversnapshot for our anachronous entity E2
    // since the target_frame for E2 will be 7 - Anachronous{frames_behind: 4} = 3
    // and upon checking, realise we have a ServerSnapshot at frame 3.
    // it will snap the component and componenthistory values for the current frame,
    // this doesn't do a rollback.
    tick(&mut app); // frame 7
    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    // NOW we should see suitable lagged data for E2.
    // current value (frame 7) should contain 7-4=3 frame-3 snapshot data.
    assert_eq!(app.world.get::<Enemy>(e2).unwrap().health, 1000);
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 96);

    assert_eq!(app.comp_val_at::<Enemy>(e2, 7).unwrap().health, 1000);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 7).unwrap().health, 96);

    tick(&mut app); // frame 8

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    assert_eq!(app.world.get::<Enemy>(e2).unwrap().health, 999);
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 95);

    assert_eq!(app.comp_val_at::<Enemy>(e2, 8).unwrap().health, 999);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 8).unwrap().health, 95);
}
