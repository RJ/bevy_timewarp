use crate::prelude::*;
use bevy::prelude::*;
/*
    Postfix Sets

    NOTE: Timewarp Postfix Systems run AFTER physics.
*/

/// Once a [`DespawnMarker`] has been around for `rollback_frames`, do the actual despawn.
/// also for new DespawnMarkers that don't have a frame yet, add one.
pub(crate) fn despawn_entities_with_elapsed_despawn_marker(
    mut q: Query<(Entity, &mut DespawnMarker)>,
    mut commands: Commands,
    game_clock: Res<GameClock>,
    timewarp_config: Res<TimewarpConfig>,
) {
    for (entity, mut marker) in q.iter_mut() {
        if marker.0.is_none() {
            marker.0 = Some(game_clock.frame());
            continue;
        }
        if (marker.0.expect("Despawn marker should have a frame!")
            + timewarp_config.rollback_window)
            == game_clock.frame()
        {
            debug!(
                "💀 Doing actual despawn of {entity:?} at frame {:?}",
                game_clock.frame()
            );
            commands.entity(entity).despawn_recursive();
        }
    }
}
