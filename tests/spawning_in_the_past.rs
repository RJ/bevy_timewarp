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
fn spawning_in_the_past() {
    let mut app = setup_test_app();

    // bevy_mod_debugdump::print_schedule_graph(&mut app, FixedUpdate);
    // panic!("EXIIIIIIT");

    app.register_rollback::<Enemy>();
    // warn!("REG ROLLBACK COMPLETE");

    // // bevy_mod_debugdump::print_schedule_graph(&mut app, FixedUpdate);
    // bevy_mod_debugdump::schedule_graph_dot(
    //     &mut app,
    //     FixedUpdate,
    //     &bevy_mod_debugdump::schedule_graph::settings::Settings {
    //         ambiguity_enable: true,
    //         ..default()
    //     },
    // );
    // panic!("EXIIIIIIT");

    app.add_systems(
        FixedUpdate,
        (inc_frame, take_damage, log_all)
            .chain()
            .in_set(TimewarpTestSets::GameLogic),
    );

    // tick(&mut app);

    // panic!("XXX");
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

    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 6);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 4).unwrap().health, 6);

    // spawn E2 and E3 in the past, both on frame 2.
    // only timewarp-registered components can be spawned into the past,
    // other components are spawned in the present frame.
    let e2 = app
        .world
        .spawn((
            EntName {
                name: "E2".to_owned(),
            },
            InsertComponentAtFrame::new(2, Enemy { health: 100 }),
        ))
        .id();

    let e3 = app
        .world
        .spawn((
            EntName {
                name: "E3".to_owned(),
            },
            InsertComponentAtFrame::new(2, Enemy { health: 1000 }),
        ))
        .id();

    tick(&mut app); // frame 5 - will trigger rollback

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    assert!(app.comp_val_at::<Enemy>(e2, 1).is_none());
    assert_eq!(app.comp_val_at::<Enemy>(e2, 2).unwrap().health, 100);
    assert_eq!(app.comp_val_at::<Enemy>(e2, 3).unwrap().health, 99);
    assert_eq!(app.comp_val_at::<Enemy>(e2, 4).unwrap().health, 98);
    assert_eq!(app.comp_val_at::<Enemy>(e2, 5).unwrap().health, 97);

    assert!(app.comp_val_at::<Enemy>(e3, 1).is_none());
    assert_eq!(app.comp_val_at::<Enemy>(e3, 2).unwrap().health, 1000);
    assert_eq!(app.comp_val_at::<Enemy>(e3, 5).unwrap().health, 997);

    let e4 = app
        .world
        .spawn((
            EntName {
                name: "E4".to_owned(),
            },
            InsertComponentAtFrame::new(3, Enemy { health: 1000 }),
        ))
        .id();

    tick(&mut app); // frame 6

    assert!(app.comp_val_at::<Enemy>(e4, 2).is_none());
    assert_eq!(app.comp_val_at::<Enemy>(e4, 3).unwrap().health, 1000);
    assert_eq!(app.comp_val_at::<Enemy>(e4, 6).unwrap().health, 997);

    // check e2 and e3 still the same

    assert!(app.comp_val_at::<Enemy>(e2, 1).is_none());
    assert_eq!(app.comp_val_at::<Enemy>(e2, 2).unwrap().health, 100);
    assert_eq!(app.comp_val_at::<Enemy>(e2, 3).unwrap().health, 99);
    assert_eq!(app.comp_val_at::<Enemy>(e2, 4).unwrap().health, 98);
    assert_eq!(app.comp_val_at::<Enemy>(e2, 5).unwrap().health, 97);

    assert!(app.comp_val_at::<Enemy>(e3, 1).is_none());
    assert_eq!(app.comp_val_at::<Enemy>(e3, 2).unwrap().health, 1000);
    assert_eq!(app.comp_val_at::<Enemy>(e3, 5).unwrap().health, 997);
}

#[test]
fn spawning_in_the_past_with_ss_partial_updates() {
    let mut app = setup_test_app();

    // this test modifies distinct things in the past at two different frames during the same tick, so:
    {
        let mut tw_config = app.world.get_resource_mut::<TimewarpConfig>().unwrap();
        tw_config.set_consolidation_strategy(RollbackConsolidationStrategy::Oldest);
    }

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
    tick(&mut app); // frame 2
    tick(&mut app); // frame 3
    tick(&mut app); // frame 4

    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 6);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 4).unwrap().health, 6);

    let _e2 = app
        .world
        .spawn((
            EntName {
                name: "E2".to_owned(),
            },
            InsertComponentAtFrame::new(2, Enemy { health: 100 }),
        ))
        .id();

    let mut ss = app.world.get_mut::<ServerSnapshot<Enemy>>(e1).unwrap();
    ss.insert(3, Enemy { health: 1000 }).unwrap();

    tick(&mut app); // frame 5 - will trigger rollback

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );
}
