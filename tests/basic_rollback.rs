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
fn basic_rollback() {
    let mut app = setup_test_app();

    app.register_rollback::<Enemy>();

    // the full game loop, including networking, rendering, etc.
    // runs when a rollback is NOT in progress.
    app.add_systems(
        FixedUpdate,
        (inc_frame, take_damage, log_all)
            .chain()
            .in_set(TimewarpTestSets::GameLogic)
            .run_if(not(resource_exists::<Rollback>())),
    );
    // the core simulation-only game loop, for running during a rollback
    app.add_systems(
        FixedUpdate,
        (inc_frame, take_damage, log_all)
            .chain()
            .in_set(TimewarpTestSets::GameLogic)
            .run_if(resource_exists::<Rollback>()),
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
            Enemy { health: 3 },
            EntName {
                name: "E2".to_owned(),
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
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 9);
    assert_eq!(app.world.get::<Enemy>(e2).unwrap().health, 2);
    // first tick after spawning, the timewarp components should have been added:
    assert!(app.world.get::<ComponentHistory<Enemy>>(e1).is_some());
    assert!(app.world.get::<ComponentHistory<Enemy>>(e2).is_some());
    assert!(app.world.get::<ServerSnapshot<Enemy>>(e1).is_some());
    assert!(app.world.get::<ServerSnapshot<Enemy>>(e2).is_some());
    // and contain the correct values from this frame:
    // let ch_e1 = app.world.get::<ComponentHistory<Enemy>>(e1).unwrap().values.get(1);
    let ch_e1 = app.comp_val_at::<Enemy>(e1, 1);
    assert!(ch_e1.is_some());
    assert_eq!(ch_e1.unwrap().health, 9);

    let ch_e2 = app.comp_val_at::<Enemy>(e2, 1);
    assert!(ch_e2.is_some());
    assert_eq!(ch_e2.unwrap().health, 2);

    tick(&mut app); // frame 2
    tick(&mut app); // frame 3
    tick(&mut app); // frame 4

    // we just simulated frame 4
    let gc = app.world.get_resource::<GameClock>().unwrap();
    assert_eq!(gc.frame(), 4);

    // by now, these should be current values
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 6);
    assert_eq!(app.world.get::<Enemy>(e2).unwrap().health, -1);

    // verify that ComponentHistory is storing values for past frames:
    let ch_e1 = app.comp_val_at::<Enemy>(e1, 2);
    assert!(ch_e1.is_some());
    assert_eq!(ch_e1.unwrap().health, 8);

    let ch_e1 = app.comp_val_at::<Enemy>(e1, 3);
    assert!(ch_e1.is_some());
    assert_eq!(ch_e1.unwrap().health, 7);

    let ch_e1 = app.comp_val_at::<Enemy>(e1, 4);
    assert!(ch_e1.is_some());
    assert_eq!(ch_e1.unwrap().health, 6);

    let ch_e2 = app.comp_val_at::<Enemy>(e2, 3);
    assert!(ch_e2.is_some());
    assert_eq!(ch_e2.unwrap().health, 0);

    let ch_e2 = app.comp_val_at::<Enemy>(e2, 4);
    assert!(ch_e2.is_some());
    assert_eq!(ch_e2.unwrap().health, -1);

    // we just simulated frame 4..
    // let's pretend during frame 5 we get a message from the server saying that on frame 2, mister E2
    // ate a powerup, changing his health to 100.
    // our app's netcode would insert the authoritative (slightly outdated) values into ServerSnapshots:

    let mut ss_e2 = app.world.get_mut::<ServerSnapshot<Enemy>>(e2).unwrap();
    ss_e2.insert(2, Enemy { health: 100 }).unwrap();

    // this message will be processed in the next tick - frame 5.

    tick(&mut app); // frame 5

    // frame 5 should run normally, then rollback systems will run, effect a rollback,
    // and resimulate from f2
    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    // E2 health will be set to 100 at frame 2, then resimulation will happen starting at frame 3
    // so frame 3 E2's health should be 99 instead of 0, which it was pre-rollback.

    let ch_e2 = app.comp_val_at::<Enemy>(e2, 2);
    assert!(ch_e2.is_some());
    assert_eq!(ch_e2.unwrap().health, 100);

    let ch_e2 = app.comp_val_at::<Enemy>(e2, 3);
    assert!(ch_e2.is_some());
    assert_eq!(ch_e2.unwrap().health, 99);

    // meanwhile, E1's health should be the same as before rollback for resimulated frames:
    let ch_e1 = app.comp_val_at::<Enemy>(e1, 3);
    assert!(ch_e1.is_some());
    assert_eq!(ch_e1.unwrap().health, 7);

    // resimulation should have brought us back to frame 5.
    let gc = app.world.get_resource::<GameClock>().unwrap();
    assert_eq!(gc.frame(), 5);

    // frame 2 health was 100,
    // frame 3 -> 99
    // frame 4 -> 98
    // frame 5 -> 97
    let ch_e2 = app.comp_val_at::<Enemy>(e2, 5);
    assert!(ch_e2.is_some());
    assert_eq!(ch_e2.unwrap().health, 97);
    assert_eq!(app.world.get::<Enemy>(e2).unwrap().health, 97);

    tick(&mut app); // frame 6

    // should have been a normal frame, no more rollbacks:
    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    let ch_e2 = app.comp_val_at::<Enemy>(e2, 6);
    assert!(ch_e2.is_some());
    assert_eq!(ch_e2.unwrap().health, 96);

    tick(&mut app); // frame 7

    assert_eq!(app.comp_val_at::<Enemy>(e2, 7).unwrap().health, 95);
    assert_eq!(app.world.get::<Enemy>(e2).unwrap().health, 95);

    // now lets test what happens if we update the server snapshot with what we know to be identical
    // values to the the client simulation, to represent a lovely deterministic simulation with no errors

    let mut ss_e2 = app.world.get_mut::<ServerSnapshot<Enemy>>(e2).unwrap();
    // we know from the asserts above that health of e2 was 97 at frame 5.
    // so lets make the server confirm that:
    ss_e2.insert(5, Enemy { health: 97 });

    tick(&mut app); // frame 8, potential rollback

    // but no  - our prediction matches the snapshot so it didn't roll back.
    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    assert_eq!(app.comp_val_at::<Enemy>(e2, 8).unwrap().health, 94);
    assert_eq!(app.comp_val_at::<Enemy>(e2, 7).unwrap().health, 95);

    assert_eq!(app.world.get::<Enemy>(e2).unwrap().health, 94);
}
