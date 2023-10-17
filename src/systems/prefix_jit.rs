/*
    NOTE: Timewarp Prefix Systems run at the top of FixedUpdate:
        * RIGHT BEFORE THE GameClock IS INCREMENTED.
        * Before the game simulation loop
        * Before Physics

*/
use crate::prelude::*;
use bevy::prelude::*;

pub(crate) fn apply_jit_ss<T: TimewarpComponent>(
    q: Query<
        (Entity, &ServerSnapshot<T>),
        Changed<ServerSnapshot<T>>, // this includes Added<>
    >,
    game_clock: Res<GameClock>,
    mut commands: Commands,
) {
    // return;
    // for (entity, server_snapshot) in q.iter() {
    //     let snap_frame = server_snapshot.values.newest_frame();

    //     if snap_frame != **game_clock || snap_frame == 0 {
    //         continue;
    //     }

    //     // the value in the SS that we are concerned with, which may possibly trigger a rollback:
    //     let comp_from_snapshot = server_snapshot
    //         .at_frame(snap_frame)
    //         .expect("snap_frame must have a value here");
    //     debug!("Inserting latecomer {entity:?} {comp_from_snapshot:?} @ {snap_frame}");
    //     commands.entity(entity).insert(comp_from_snapshot.clone());
    // }
}

/// moves from ICAF wrapper to SS. inserts directly if frames match.
pub(crate) fn apply_jit_icafs<T: TimewarpComponent, const CORRECTION_LOGGING: bool>(
    mut q: Query<(
        Entity,
        &InsertComponentAtFrame<T>,
        Option<&mut TimewarpStatus>,
    )>,
    mut commands: Commands,
    timewarp_config: Res<TimewarpConfig>,
    game_clock: Res<GameClock>,
) {
    // return;

    // for (entity, icaf, opt_tw_status) in q.iter_mut() {
    //     let mut ent_cmd = commands.entity(entity);

    //     if let Some(mut tw_status) = opt_tw_status {
    //         tw_status.set_snapped_at(icaf.frame);
    //     } else {
    //         ent_cmd.insert(TimewarpStatus::new(icaf.frame));
    //     }
    //     debug!("apply_jit_icafs {entity:?} {icaf:?} moving icaf -> ss");

    //     let mut ss =
    //         ServerSnapshot::<T>::with_capacity(timewarp_config.rollback_window as usize * 60); // TODO yuk
    //     ss.insert(icaf.frame, icaf.component.clone());

    //     let mut ch = ComponentHistory::<T>::with_capacity(
    //         timewarp_config.rollback_window as usize,
    //         icaf.frame,
    //         icaf.component.clone(),
    //         &entity,
    //     );
    //     if CORRECTION_LOGGING {
    //         ch.enable_correction_logging();
    //     }

    //     let tw_status = TimewarpStatus::new(icaf.frame);

    //     ent_cmd
    //         .remove::<InsertComponentAtFrame<T>>()
    //         .insert((ch, ss, tw_status));

    //     if icaf.frame == **game_clock {
    //         debug!(
    //             "Inserting latecomer {entity:?} ss:added {icaf:?} @ {}",
    //             icaf.frame
    //         );
    //         ent_cmd.insert(icaf.component.clone());
    //     }
    // }
}
