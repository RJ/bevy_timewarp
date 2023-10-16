/*
    NOTE: Timewarp Prefix Systems run at the top of FixedUpdate:
        * RIGHT BEFORE THE GameClock IS INCREMENTED.
        * Before the game simulation loop
        * Before Physics

*/
use crate::prelude::*;
use bevy::prelude::*;

/// Don't insert ICAFs if the SS exists, use the SS.
/// can probably support this later, but keeps things simpler for now.
pub(crate) fn detect_misuse_of_icaf<T: TimewarpComponent>(
    q: Query<(Entity, &ServerSnapshot<T>, &InsertComponentAtFrame<T>)>,
) {
    for (e, _ss, icaf) in q.iter() {
        panic!("ICAF and SS exist on {e:?} {icaf:?}");
    }
}

/// If a new snapshot was added to SS, we may need to initiate a rollback to that frame
pub(crate) fn trigger_rollback_when_snapshot_added<T: TimewarpComponent>(
    mut q: Query<
        (
            Entity,
            &ServerSnapshot<T>,
            &mut ComponentHistory<T>,
            &mut TimewarpStatus,
        ),
        Changed<ServerSnapshot<T>>, // this includes Added<>
    >,
    game_clock: Res<GameClock>,
    mut rb_ev: ResMut<Events<RollbackRequest>>,
    config: Res<TimewarpConfig>,
) {
    for (entity, server_snapshot, mut comp_hist, mut tw_status) in q.iter_mut() {
        let snap_frame = server_snapshot.values.newest_frame();

        if snap_frame == 0 {
            continue;
        }

        // shouldn't really be getting snapshots from the future. clients are supposed to be ahead.
        // TODO will this ever actually be applied to the component?
        if snap_frame >= **game_clock {
            continue;
        }

        tw_status.set_snapped_at(snap_frame);

        // the value in the SS that we are concerned with, which may possibly trigger a rollback:
        let comp_from_snapshot = server_snapshot
            .at_frame(snap_frame)
            .expect("snap_frame must have a value here");

        // check if our historical value for the snap_frame is the same as what snapshot says
        // because if they match, we predicted successfully, and there's no need to rollback.
        if let Some(stored_comp_val) = comp_hist.at_frame(snap_frame) {
            if !config.forced_rollback() && *stored_comp_val == *comp_from_snapshot {
                // a correct prediction, no need to rollback. hooray!
                info!("skipping rollback 🎖️ {entity:?} {stored_comp_val:?}");
                continue;
            }
        }

        // new:
        comp_hist.insert(snap_frame, comp_from_snapshot.clone(), &entity);

        debug!(
            "Triggering rollback due to snapshot. {entity:?} snap_frame: {snap_frame} {}",
            comp_hist.type_name()
        );

        // trigger a rollback using the frame we just added authoritative values for
        rb_ev.send(RollbackRequest(snap_frame));
    }
}

/// if an ICAF was inserted, we may need to rollback.
pub(crate) fn trigger_rollback_when_icaf_added<
    T: TimewarpComponent,
    const CORRECTION_LOGGING: bool,
>(
    q: Query<(Entity, &InsertComponentAtFrame<T>), Added<InsertComponentAtFrame<T>>>,
    mut commands: Commands,
    timewarp_config: Res<TimewarpConfig>,
    game_clock: Res<GameClock>,
    mut rb_ev: ResMut<Events<RollbackRequest>>,
) {
    for (e, icaf) in q.iter() {
        if icaf.frame >= **game_clock {
            continue;
        }

        // insert the timewarp components
        let tw_status = TimewarpStatus::new(icaf.frame);
        let mut ch = ComponentHistory::<T>::with_capacity(
            timewarp_config.rollback_window as usize,
            icaf.frame,
        );
        if CORRECTION_LOGGING {
            ch.enable_correction_logging();
        }
        // TODO SS = yuk, sparse. use better data structure
        let mut ss =
            ServerSnapshot::<T>::with_capacity(timewarp_config.rollback_window as usize * 60);
        ss.insert(icaf.frame, icaf.component.clone());
        // (this will be applied in the ApplyComponents set next)
        commands.entity(e).insert((tw_status, ch, ss));
        info!(
            "{e:?} trigger_rollback_when_icaf_added {icaf:?} requesting rb to {}",
            icaf.frame
        );
        // trigger a rollback using the frame we just added authoritative values for
        rb_ev.send(RollbackRequest(icaf.frame));
    }
}

pub(crate) fn trigger_rollback_when_blueprint_added<T: TimewarpComponent>(
    q: Query<&AssembleBlueprintAtFrame<T>, Added<AssembleBlueprintAtFrame<T>>>,
    game_clock: Res<GameClock>,
    mut rb_ev: ResMut<Events<RollbackRequest>>,
) {
    for abaf in q.iter() {
        // this system runs in preup, and the clock is about to increment.
        // so we need to be abaf.frame-1 in preup for it to be unwrapped
        let snap_frame = abaf.frame; //.saturating_sub(1);
        if snap_frame < **game_clock {
            info!(
                "{game_clock:?} TRIGGERING ROLLBACK to {snap_frame} due to added blueprint: {abaf:?}"
            );
            rb_ev.send(RollbackRequest(snap_frame));
        }
    }
}

/// potentially-concurrent systems request rollbacks by writing a request
/// to the Events<RollbackRequest>, which we drain and use the smallest
/// frame that was requested - ie, covering all requested frames.
pub(crate) fn consolidate_rollback_requests(
    mut rb_events: ResMut<Events<RollbackRequest>>,
    mut commands: Commands,
    game_clock: Res<GameClock>,
) {
    let mut rb_frame: FrameNumber = 0;
    // NB: a manually managed event queue, which we drain here
    for ev in rb_events.drain() {
        if rb_frame == 0 || ev.0 < rb_frame {
            rb_frame = ev.0;
        }
    }
    if rb_frame == 0 {
        return;
    }
    // The RollbackRequest is the frame new authoritative data was added for.
    // we need to load in data for the previous frame the resimulate the next frame.
    commands.insert_resource(Rollback::new(rb_frame, game_clock.frame()));
}
