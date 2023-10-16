/*
    NOTE: Timewarp Prefix Systems run at the top of FixedUpdate:
        * RIGHT BEFORE THE GameClock IS INCREMENTED.
        * Before the game simulation loop
        * Before Physics

*/
use crate::prelude::*;
use bevy::prelude::*;

/// If a new snapshot was added to SS, we may need to initiate a rollback to that frame
pub(crate) fn apply_jit_ss<T: TimewarpComponent>(
    q: Query<
        (Entity, &ServerSnapshot<T>),
        Changed<ServerSnapshot<T>>, // this includes Added<>
    >,
    game_clock: Res<GameClock>,
    mut rb_ev: ResMut<Events<RollbackRequest>>,
    config: Res<TimewarpConfig>,
    mut commands: Commands,
) {
    for (entity, server_snapshot) in q.iter() {
        let snap_frame = server_snapshot.values.newest_frame();

        if snap_frame != **game_clock || snap_frame == 0 {
            continue;
        }

        // the value in the SS that we are concerned with, which may possibly trigger a rollback:
        let comp_from_snapshot = server_snapshot
            .at_frame(snap_frame)
            .expect("snap_frame must have a value here");
        info!("Inserting latecomer {entity:?} {comp_from_snapshot:?} @ {snap_frame}");
        commands.entity(entity).insert(comp_from_snapshot.clone());
    }
}

pub(crate) fn apply_jit_icafs<T: TimewarpComponent, const CORRECTION_LOGGING: bool>(
    mut q: Query<(
        Entity,
        &InsertComponentAtFrame<T>,
        // NOTE the timewarp components might not have been added if this is a first-timer entity
        // which is why they have to be Option<> here, in case we need to insert them.
        Option<&mut ComponentHistory<T>>,
        Option<&mut ServerSnapshot<T>>,
        Option<&mut TimewarpStatus>,
    )>,
    mut commands: Commands,
    timewarp_config: Res<TimewarpConfig>,
    game_clock: Res<GameClock>,
) {
    for (entity, icaf, opt_ch, opt_ss, opt_tw_status) in q.iter_mut() {
        assert!(
            opt_ch.is_some() == opt_ss.is_some() && opt_ss.is_some() == opt_tw_status.is_some()
        );

        if let Some(mut ss) = opt_ss {
            commands
                .entity(entity)
                .remove::<InsertComponentAtFrame<T>>();
            ss.insert(icaf.frame, icaf.component.clone());
            opt_tw_status.unwrap().set_snapped_at(icaf.frame);
        } else {
            let mut ss =
                ServerSnapshot::<T>::with_capacity(timewarp_config.rollback_window as usize * 60); // TODO yuk
            ss.insert(icaf.frame, icaf.component.clone());

            let mut ch = ComponentHistory::<T>::with_capacity(
                timewarp_config.rollback_window as usize,
                icaf.frame,
            );
            if CORRECTION_LOGGING {
                ch.enable_correction_logging();
            }

            let tw_status = TimewarpStatus::new(icaf.frame);

            commands
                .entity(entity)
                .remove::<InsertComponentAtFrame<T>>()
                .insert((ch, ss, tw_status));
        }
    }
}

/// Blueprint components stay wrapped up until their target frame, then we unwrap them
/// so the assembly systems can decorate them with various other components at that frame.
pub(crate) fn unwrap_blueprints_at_target_frame<T: TimewarpComponent>(
    q: Query<(Entity, &AssembleBlueprintAtFrame<T>)>,
    mut commands: Commands,
    game_clock: Res<GameClock>,
    rb: Res<Rollback>,
) {
    for (e, abaf) in q.iter() {
        if abaf.frame != **game_clock {
            continue;
        }
        info!(
            "🎁 Unwrapping {abaf:?} @ {game_clock:?} {rb:?} {}",
            std::any::type_name::<T>()
        );
        commands
            .entity(e)
            .insert(abaf.component.clone())
            .remove::<AssembleBlueprintAtFrame<T>>();
    }
}
