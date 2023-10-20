use std::cmp::Ordering;

/*
    NOTE: Timewarp Prefix Systems run at the top of FixedUpdate:
        * RIGHT BEFORE THE GameClock IS INCREMENTED.
        * Before the game simulation loop
        * Before Physics

*/
use crate::prelude::*;
use bevy::prelude::*;

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
                rb_stats.range_faults += 1;
                // probably FrameTooOld.
                panic!(
                    "{err:?} {entity:?} apply_snapshots_and_maybe_rollback({}) - skipping",
                    comp_hist.type_name()
                );
                // we can't rollback to this
                // this is bad.
                // continue;
            }
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
            tw_status.increment_rollback_triggers();
        }
    }
}

/// Move ICAF data to the SS and add SS, because it's missing.
///
/// if an ICAF was inserted, we may need to rollback.
///
pub(crate) fn unpack_icafs_adding_tw_components<
    T: TimewarpComponent,
    const CORRECTION_LOGGING: bool,
>(
    mut q: Query<
        (
            Entity,
            &InsertComponentAtFrame<T>,
            Option<&mut TimewarpStatus>,
        ),
        (
            Added<InsertComponentAtFrame<T>>,
            Without<NoRollback>,
            Without<ServerSnapshot<T>>,
            Without<ComponentHistory<T>>,
        ),
    >,
    mut commands: Commands,
    timewarp_config: Res<TimewarpConfig>,
    game_clock: Res<GameClock>,
    mut rb_ev: ResMut<Events<RollbackRequest>>,
) {
    for (e, icaf, opt_twstatus) in q.iter_mut() {
        // insert the timewarp components
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

        match icaf.frame.cmp(&game_clock.frame()) {
            // if frames match, we want it inserted this frame but not rolled back
            // since it has arrived just in time.
            Ordering::Equal => {
                if let Some(mut tw_status) = opt_twstatus {
                    tw_status.set_snapped_at(icaf.frame);
                } else {
                    let mut tw_status = TimewarpStatus::new(icaf.frame);
                    tw_status.set_snapped_at(icaf.frame);
                    commands.entity(e).insert(tw_status);
                }
                commands
                    .entity(e)
                    .insert((ch, ss, icaf.component.clone()))
                    .remove::<InsertComponentAtFrame<T>>();
            }
            // needs insertion in the past, so request a rollback.
            Ordering::Less => {
                debug!(
                    "{e:?} Requesting rolllback when unpacking: {icaf:?} rb to {}",
                    icaf.frame + 1
                );
                if let Some(mut tw_status) = opt_twstatus {
                    tw_status.increment_rollback_triggers();
                    tw_status.set_snapped_at(icaf.frame);
                } else {
                    let mut tw_status = TimewarpStatus::new(icaf.frame);
                    tw_status.increment_rollback_triggers();
                    tw_status.set_snapped_at(icaf.frame);
                    commands.entity(e).insert(tw_status);
                }
                commands
                    .entity(e)
                    .insert((ch, ss))
                    .remove::<InsertComponentAtFrame<T>>();
                rb_ev.send(RollbackRequest::resimulate_this_frame_onwards(
                    icaf.frame + 1,
                ));
            }
            Ordering::Greater => {
                // clients are supposed to be ahead, so we don't really expect to get updates for
                // future frames. We'll store it but can't rollback to future.
                commands
                    .entity(e)
                    .insert((ch, ss))
                    .remove::<InsertComponentAtFrame<T>>();
            }
        }
    }
}

/// Move ICAF data to the existing SS
///
/// if an ICAF was inserted, we may need to rollback.
///
pub(crate) fn unpack_icafs_into_tw_components<
    T: TimewarpComponent,
    const CORRECTION_LOGGING: bool,
>(
    mut q: Query<
        (
            Entity,
            &InsertComponentAtFrame<T>,
            &mut ServerSnapshot<T>,
            &mut ComponentHistory<T>,
            &mut TimewarpStatus,
        ),
        (Added<InsertComponentAtFrame<T>>, Without<NoRollback>),
    >,
    mut commands: Commands,
    game_clock: Res<GameClock>,
    mut rb_ev: ResMut<Events<RollbackRequest>>,
) {
    for (e, icaf, mut ss, mut ch, mut tw_status) in q.iter_mut() {
        ch.insert(icaf.frame, icaf.component.clone(), &e)
            .expect("Couldn't insert ICAF to CH");
        ss.insert(icaf.frame, icaf.component.clone())
            .expect("Couldn't insert ICAF to SS");

        info!("Alive ranges for {icaf:?} = {:?}", ch.alive_ranges);

        match icaf.frame.cmp(&game_clock.frame()) {
            // if frames match, we want it inserted this frame but not rolled back
            // since it has arrived just in time.
            Ordering::Equal => {
                commands
                    .entity(e)
                    .insert(icaf.component.clone())
                    .remove::<InsertComponentAtFrame<T>>();
            }
            // needs insertion in the past, so request a rollback.
            Ordering::Less => {
                debug!(
                    "{e:?} Requesting rolllback when unpacking: {icaf:?} rb to {}",
                    icaf.frame + 1
                );
                tw_status.increment_rollback_triggers();
                commands.entity(e).remove::<InsertComponentAtFrame<T>>();
                rb_ev.send(RollbackRequest::resimulate_this_frame_onwards(
                    icaf.frame + 1,
                ));
            }
            Ordering::Greater => {
                // clients are supposed to be ahead, so we don't really expect to get updates for
                // future frames. We'll store it but can't rollback to future.
                commands.entity(e).remove::<InsertComponentAtFrame<T>>();
            }
        }
    }
}

pub(crate) fn request_rollback_for_blueprints<T: Component + std::fmt::Debug + Clone>(
    mut q: Query<
        (
            Entity,
            &AssembleBlueprintAtFrame<T>,
            Option<&mut TimewarpStatus>,
        ),
        Added<AssembleBlueprintAtFrame<T>>,
    >,
    game_clock: Res<GameClock>,
    mut rb_ev: ResMut<Events<RollbackRequest>>,
    mut commands: Commands,
) {
    for (entity, abaf, opt_twstatus) in q.iter_mut() {
        let snap_frame = abaf.frame;
        // if frames == match, we want it inserted this frame but not rolled back.
        // don't do this here, the blueprint unpacking fn does this even during rollback.
        // all we have to do is trigger a rollback, and it'll be unpacked for us.
        if snap_frame < **game_clock {
            debug!(
                "{game_clock:?} {entity:?} Requesting rollback for blueprint with snap_frame:{snap_frame} - {abaf:?}"
            );
            if let Some(mut tws) = opt_twstatus {
                tws.increment_rollback_triggers();
            } else {
                let mut tws = TimewarpStatus::new(snap_frame);
                tws.increment_rollback_triggers();
                commands.entity(entity).insert(tws);
            }
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
    conf: Res<TimewarpConfig>,
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
       as of october 2023, then it's safe to rollback to the most recent frame.

       if we get partial updates per packet - ie not all entities included per tick - then we need
       to rollback to the oldest requested frame, or we might miss data for entities that were
       included in the first packet (@95) but not in the second (@96).

       if've not really tested the second scenario yet, because replicon uses whole-world updates atm.
    */
    let mut rb_frame: FrameNumber = 0;
    // NB: a manually managed event queue, which we drain here
    for ev in rb_events.drain() {
        match conf.consolidation_strategy() {
            RollbackConsolidationStrategy::Newest => {
                if rb_frame == 0 || ev.frame() > rb_frame {
                    rb_frame = ev.frame();
                }
            }
            RollbackConsolidationStrategy::Oldest => {
                if rb_frame == 0 || ev.frame() < rb_frame {
                    rb_frame = ev.frame();
                }
            }
        }
    }
    commands.insert_resource(Rollback::new(rb_frame, game_clock.frame()));
}
