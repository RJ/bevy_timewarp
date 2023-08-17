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
fn despawn_markers() {
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

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        0
    );

    tick(&mut app); // frame 1
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 9);

    tick(&mut app); // frame 2
    tick(&mut app); // frame 3

    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 7);

    assert_eq!(app.comp_val_at::<Enemy>(e1, 1).unwrap().health, 9);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 2).unwrap().health, 8);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 3).unwrap().health, 7);

    // this is how we despawn:
    // it removes all the components in the same frame, then waits until the rollback_window has
    // elapsed in order to do the actual despawn.
    let despawn_frame = 4;
    app.world
        .entity_mut(e1)
        .insert(DespawnMarker::for_frame(despawn_frame));

    tick(&mut app); // frame 4

    assert!(
        app.world.get_entity(e1).is_some(),
        "entity should still exist"
    );
    assert!(
        app.world.get::<Enemy>(e1).is_none(),
        "Enemy component should be missing"
    );

    for _ in 0..TEST_ROLLBACK_WINDOW {
        tick(&mut app);
    }
    assert!(
        app.world.get_entity(e1).is_none(),
        "entity should be gone by now"
    );
}

#[test]
fn despawn_revival_during_rollback() {
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

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        0
    );

    tick(&mut app); // frame 1
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 9);

    tick(&mut app); // frame 2
    tick(&mut app); // frame 3

    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 7);

    assert_eq!(app.comp_val_at::<Enemy>(e1, 1).unwrap().health, 9);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 2).unwrap().health, 8);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 3).unwrap().health, 7);

    // this is how we despawn:
    // it removes all the components in the same frame, then waits until the rollback_window has
    // elapsed in order to do the actual despawn.
    // this won't actually appear to be despawned until frame 5, because timewarp stuff
    // runs at the end after game logic. So queries in frame 4 will see it, then the
    // timewarp systems run after game logic, and remove components, which will be done by f5.
    // TODO perhaps we want to move the despawn systems to a timewarp header set before game logic?
    //      this would make the behaviour seem more sane? hmm.
    let despawn_frame = 4;
    app.world
        .entity_mut(e1)
        .insert(DespawnMarker::for_frame(despawn_frame));

    tick(&mut app); // frame 4

    assert!(
        app.world.get_entity(e1).is_some(),
        "entity should still exist"
    );
    assert!(
        app.world.get::<Enemy>(e1).is_none(),
        "Enemy component should be missing"
    );

    // generate a rollback that should revive the component temporarily
    let mut ss_e2 = app.world.get_mut::<ServerSnapshot<Enemy>>(e1).unwrap();
    ss_e2.insert(2, Enemy { health: 100 });

    tick(&mut app); // tick 1/rollback_window until despawn

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    assert!(
        app.world.get_entity(e1).is_some(),
        "entity should still exist"
    );
    assert!(
        app.world.get::<Enemy>(e1).is_none(),
        "Enemy component should still be missing"
    );

    // even though the Enemy component doesn't exist on e1 now, we can see a rollback happened
    // because the buffered older values have changed in accordance with the ServerSnapshot:
    assert_eq!(app.comp_val_at::<Enemy>(e1, 2).unwrap().health, 100);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 3).unwrap().health, 99);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 4).unwrap().health, 98);

    assert!(app.comp_val_at::<Enemy>(e1, 5).is_none());

    // remaining ticks until actual despawn happens
    for _ in 1..TEST_ROLLBACK_WINDOW {
        tick(&mut app);
    }
    assert!(
        app.world.get_entity(e1).is_none(),
        "entity should be gone by now"
    );
}
