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
fn rollback_over_new_spawn() {
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
    tick(&mut app); // frame 2
    tick(&mut app); // frame 3
    tick(&mut app); // frame 4

    // we just simulated frame 4
    let gc = app.world.get_resource::<GameClock>().unwrap();
    assert_eq!(gc.frame(), 4);

    // by now, these should be current values
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 6);
    assert_eq!(app.world.get::<Enemy>(e2).unwrap().health, -1);

    let e3 = app
        .world
        .spawn((
            Enemy { health: 3000 },
            EntName {
                name: "E3".to_owned(),
            },
        ))
        .id();

    tick(&mut app); // frame 5

    let mut e1mut = app.world.entity_mut(e1);
    e1mut
        .insert_component_at_frame(3, &Enemy { health: 9999 })
        .unwrap();

    tick(&mut app); // frame 6 - rb

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    assert!(app.world.get_entity(e3).is_some(), "e3 should still exist");
    assert!(
        app.comp_val_at::<Enemy>(e3, 6).is_some(),
        "e3's Enemy component should exist"
    );
    assert_eq!(app.comp_val_at::<Enemy>(e3, 6).unwrap().health, 2998);
}
