use bevy::prelude::*;
use bevy_timewarp::prelude::*;

mod test_utils;
use test_utils::*;

fn inc_frame(mut game_clock: ResMut<GameClock>, rb: Option<Res<Rollback>>) {
    game_clock.advance(1);
    info!("FRAME --> {:?} rollback:{rb:?}", game_clock.frame());
}

fn take_damage(mut q: Query<(Entity, &mut Enemy, &EntName, Option<&Shield>)>) {
    for (entity, mut enemy, name, opt_shield) in q.iter_mut() {
        if opt_shield.is_none() {
            enemy.health -= 1;
            info!("{entity:?} took 1 damage -> {enemy:?} {name:?}");
        } else {
            info!("{entity:?} took NO damage due to having a shield -> {enemy:?} {name:?}");
        }
    }
}

fn log_all(game_clock: Res<GameClock>, q: Query<(Entity, &Enemy, &EntName, Option<&Shield>)>) {
    for tuple in q.iter() {
        info!("f:{:?} {tuple:?}", game_clock.frame());
    }
}

#[derive(Component, Debug, Clone)]
struct Shield;

/// in this test a server update adds a Shield entity in the past, which prevents the
/// enemy from taking damage. We later add the Shield back again.
#[test]
fn component_add_and_remove() {
    let mut app = setup_test_app();

    app.register_rollback::<Enemy>();
    app.register_rollback::<Shield>();

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

    // first tick after spawning, the timewarp components should have been added:
    assert!(app.world.get::<ComponentHistory<Enemy>>(e1).is_some());

    tick(&mut app); // frame 2 health -> 8
    tick(&mut app); // frame 3 health -> 7
    tick(&mut app); // frame 4 health -> 6

    assert_eq!(app.comp_val_at::<Enemy>(e1, 3).unwrap().health, 7);

    // buffered and actual values should of course match for this frame:
    assert_eq!(app.comp_val_at::<Enemy>(e1, 4).unwrap().health, 6);
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 6);

    // we just simulated frame 4
    let gc = app.world.get_resource::<GameClock>().unwrap();
    assert_eq!(gc.frame(), 4);

    // server reports E1 acquired a shield on frame 3
    let shield_added_frame = 3 as FrameNumber;
    let shield_comp = Shield;
    // adding a component for an historical frame:
    let historical_component = InsertComponentAtFrame::new(shield_added_frame, shield_comp);
    app.world.entity_mut(e1).insert(historical_component);

    tick(&mut app); // frame 5
    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    // frame 5 should run normally, then rollback systems will run, effect a rollback,
    // and resimulate from f2
    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        1
    );

    // health should not have reduced since shield was added at f3
    assert_eq!(app.comp_val_at::<Enemy>(e1, 5).unwrap().health, 7);
    
    assert_eq!(
        app.comp_val_at::<Enemy>(e1, 5).unwrap().health,
        app.comp_val_at::<Enemy>(e1, 3).unwrap().health
    );

    tick(&mut app); // frame 6
    assert_eq!(app.comp_val_at::<Enemy>(e1, 6).unwrap().health, 7);
    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 7);

    info!("removing shield");
    app.world.entity_mut(e1).remove::<Shield>();

    tick(&mut app); // frame 7
    tick(&mut app); // frame 8
    tick(&mut app); // frame 9

    assert_eq!(app.comp_val_at::<Enemy>(e1, 7).unwrap().health, 6);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 8).unwrap().health, 5);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 9).unwrap().health, 4);

    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 4);

    // now lets add back a shield at frame 8
    // this tests the following two slightly different code paths:
    // * add component at old frame where entity never had this component before
    // * add component at old frame where entity used to have this comp but doesn't atm
    app.world
        .entity_mut(e1)
        .insert(InsertComponentAtFrame::<Shield>::new(8, Shield));

    tick(&mut app); // frame 10

    assert_eq!(
        app.world
            .get_resource::<RollbackStats>()
            .unwrap()
            .num_rollbacks,
        2
    );

    assert_eq!(app.comp_val_at::<Enemy>(e1, 8).unwrap().health, 5);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 9).unwrap().health, 5);
    assert_eq!(app.comp_val_at::<Enemy>(e1, 10).unwrap().health, 5);

    assert_eq!(app.world.get::<Enemy>(e1).unwrap().health, 5);

    let gc = app.world.get_resource::<GameClock>().unwrap();
    assert_eq!(gc.frame(), 10);
}
