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

/// If a new snapshot was added to SS, we may need to initiate a rollback
pub(crate) fn apply_snapshots_and_maybe_rollback<T: TimewarpComponent>(
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
    mut commands: Commands,
    mut rb_stats: ResMut<RollbackStats>,
) {
    for (entity, server_snapshot, mut comp_hist, mut tw_status) in q.iter_mut() {
        let snap_frame = server_snapshot.values.newest_frame();

        if snap_frame == 0 {
            continue;
        }

        tw_status.set_snapped_at(snap_frame);

        // the value in the SS that we are concerned with, which may possibly trigger a rollback:
        let comp_from_snapshot = server_snapshot
            .at_frame(snap_frame)
            .expect("snap_frame must have a value here");

        // we're in preudpate, the game clock is about to be incremented.
        // so if the snap frame = current clock, we need it inserted right now without rolling back
        // in this case, we don't need to write to comp_hist either, it will happen normally at the end of the frame.
        if snap_frame == **game_clock {
            trace!("Inserting latecomer {entity:?} {comp_from_snapshot:?} @ {snap_frame}");
            commands.entity(entity).insert(comp_from_snapshot.clone());
            rb_stats.non_rollback_updates += 1;
            continue;
        }

        // check if our historical value for the snap_frame is the same as what snapshot says
        // because if they match, we predicted successfully, and there's no need to rollback.
        if let Some(stored_comp_val) = comp_hist.at_frame(snap_frame) {
            if !config.forced_rollback() && *stored_comp_val == *comp_from_snapshot {
                // a correct prediction, no need to rollback. hooray!
                trace!("skipping rollback 🎖️ {entity:?} {stored_comp_val:?}");
                continue;
            }
        }

        // need to update comp_hist, since that's where it's loaded from if we rollback.
        match comp_hist.insert(snap_frame, comp_from_snapshot.clone(), &entity) {
            Ok(()) => (),
            Err(err) => {
                // probably FrameTooOld.
                error!(
                    "{err:?} {entity:?} apply_snapshots_and_maybe_rollback({}) - skipping",
                    comp_hist.type_name()
                );
                rb_stats.range_faults += 1;
                // we can't rollback to this
                // this is bad.
                continue;
            }
        }

        if !comp_hist.alive_at_frame(snap_frame) {
            info!("Setting liveness for {snap_frame} {entity:?} {comp_from_snapshot:?} ");
            comp_hist.report_birth_at_frame(snap_frame);
            assert!(comp_hist.at_frame(snap_frame).is_some());
        }

        if snap_frame < **game_clock {
            debug!(
                "Triggering rollback due to snapshot. {entity:?} snap_frame: {snap_frame} {}",
                comp_hist.type_name()
            );

            // data for frame 100 is the post-physics value at the server, so we need it to be
            // inserted in time for the client to simulate frame 101.
            rb_ev.send(RollbackRequest::resimulate_this_frame_onwards(
                snap_frame + 1,
            ));
        }
    }
}

/// Move ICAF data to the SS.
///
/// if an ICAF was inserted, we may need to rollback.
///
pub(crate) fn unpack_icafs_and_maybe_rollback<
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
        // insert the timewarp components
        let tw_status = TimewarpStatus::new(icaf.frame);
        let mut ch = ComponentHistory::<T>::with_capacity(
            timewarp_config.rollback_window as usize,
            icaf.frame,
            icaf.component.clone(),
            &e,
        );
        if CORRECTION_LOGGING {
            ch.enable_correction_logging();
        }
        // TODO SS = yuk, sparse. use better data structure
        let mut ss =
            ServerSnapshot::<T>::with_capacity(timewarp_config.rollback_window as usize * 60);
        ss.insert(icaf.frame, icaf.component.clone()).unwrap();
        // (this will be applied in the ApplyComponents set next)
        commands
            .entity(e)
            .insert((tw_status, ch, ss))
            .remove::<InsertComponentAtFrame<T>>();

        // if frames match, we want it inserted this frame but not rolled back
        if icaf.frame == **game_clock {
            // info!("Inserting latecomer in trigger icafs: {e:?} {icaf:?}");
            commands.entity(e).insert(icaf.component.clone());
            continue;
        }

        if icaf.frame < **game_clock {
            // trigger a rollback using the frame we just added authoritative values for
            debug!(
                "{e:?} trigger_rollback_when_icaf_added {icaf:?} requesting rb to {}",
                icaf.frame + 1
            );
            rb_ev.send(RollbackRequest::resimulate_this_frame_onwards(
                icaf.frame + 1,
            ));
        }
    }
}

pub(crate) fn request_rollback_for_blueprints<T: TimewarpComponent>(
    q: Query<(Entity, &AssembleBlueprintAtFrame<T>), Added<AssembleBlueprintAtFrame<T>>>,
    game_clock: Res<GameClock>,
    mut rb_ev: ResMut<Events<RollbackRequest>>,
) {
    for (entity, abaf) in q.iter() {
        let snap_frame = abaf.frame;
        // if frames == match, we want it inserted this frame but not rolled back.
        // don't do this here, the blueprint unpacking fn does this even during rollback.
        // all we have to do is trigger a rollback, and it'll be unpacked for us.
        if snap_frame < **game_clock {
            debug!(
                "{game_clock:?} {entity:?} Requesting rollback for blueprint with snap_frame:{snap_frame} - {abaf:?}"
            );
            rb_ev.send(RollbackRequest::resimulate_this_frame_onwards(
                snap_frame + 1,
            ));
        }
    }
}

/// potentially-concurrent systems request rollbacks by writing a request
/// to the Events<RollbackRequest>, which we drain and use the smallest
/// frame that was requested - ie, covering all requested frames.
///
pub(crate) fn consolidate_rollback_requests(
    mut rb_events: ResMut<Events<RollbackRequest>>,
    mut commands: Commands,
    game_clock: Res<GameClock>,
) {
    if rb_events.is_empty() {
        return;
    }
    /*
       Say the client is in PreUpdate, with clock at 100.
       There are 2 replicon packets to process which we just read from the network in this order:
       * Updates for frame 95
       * Updates for frame 96

       Client processes first packet:  inserts values into SS for frame 95, and request rollbacks to 95+1
       Client processes second packet: inserts values into SS for frame 96, and request rollbacks to 96+1

       If we are sure we're getting entire world updates per packet – which we are with replicon
       as of october 2023, then it's safe to rollback to the most recent frame i think.

       if we get partial updates per packet - ie not all entities included per tick - then we need
       to rollback to the oldest requested frame, or we might miss data for entities that were
       included in the first packet (@95) but not in the second (@96).
    */
    // this hashmap stuff is a temporary debugging hack to detect if/when this is happening
    // don't really want or need to allocate here..
    let mut rb_reqs = bevy::utils::HashMap::<FrameNumber, u32>::new();
    let mut rb_frame: FrameNumber = 0;
    // NB: a manually managed event queue, which we drain here
    for ev in rb_events.drain() {
        *(rb_reqs.entry(ev.frame()).or_default()) += 1;
        if rb_frame == 0 || ev.frame() < rb_frame {
            rb_frame = ev.frame();
        }
    }
    // multiple frame targets requested?
    if rb_reqs.len() > 1 {
        let max_frame = rb_reqs.keys().max().unwrap();
        warn!("🎢 ROLLBACK REQS SPAN MANY FRAMES: {rb_reqs:?} rb_frame:{rb_frame} BUT changing to max_frame: {max_frame}");
        // hoping this might help limit the rollback depth when client gets bogged down.
        rb_frame = *max_frame;
    }

    commands.insert_resource(Rollback::new(rb_frame, game_clock.frame()));
}
