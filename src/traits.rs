use crate::systems::*;
use bevy::prelude::*;

use super::*;

/// This is an empty trait, used as a trait alias to make things more readable
/// see: https://www.worthe-it.co.za/blog/2017-01-15-aliasing-traits-in-rust.html
pub trait TimewarpComponent: Component + Clone + std::fmt::Debug
where
    Self: std::marker::Sized,
{
}

impl<T> TimewarpComponent for T
where
    T: Component + Clone + std::fmt::Debug,
{
    // Nothing to implement, since T already supports the other traits.
}

/// trait for registering components with the rollback system.
pub trait TimewarpTraits {
    /// register component for rollback
    fn register_rollback<T: TimewarpComponent>(&mut self) -> &mut Self;
    /// register component for rollback, and also update a TimewarpCorrection<T> component when snapping
    fn register_rollback_with_correction_logging<T: TimewarpComponent>(&mut self) -> &mut Self;
    /// register component for rollback with additional options
    fn register_rollback_with_options<T: TimewarpComponent, const CORRECTION_LOGGING: bool>(
        &mut self,
    ) -> &mut Self;
}

impl TimewarpTraits for App {
    fn register_rollback<T: TimewarpComponent>(&mut self) -> &mut Self {
        self.register_rollback_with_options::<T, false>()
    }
    fn register_rollback_with_correction_logging<T: TimewarpComponent>(&mut self) -> &mut Self {
        self.register_rollback_with_options::<T, true>()
    }
    fn register_rollback_with_options<T: TimewarpComponent, const CORRECTION_LOGGING: bool>(
        &mut self,
    ) -> &mut Self {
        self
            // we want to record frame values even if we're about to rollback -
            // we need values pre-rb to diff against post-rb versions.
            // ---
            // TimewarpSet::RecordComponentValues
            // * Runs always
            // ---
            .add_systems(
                FixedUpdate,
                (
                    add_timewarp_buffer_components::<T, CORRECTION_LOGGING>,
                    // Recording component births. this does the Added<> query, and bails if in rollback
                    // so that the Added query is refreshed.
                    record_component_birth::<T>,
                    record_component_history::<T>,
                    insert_components_at_prior_frames::<T>,
                    remove_components_from_despawning_entities::<T>
                        .after(record_component_history::<T>)
                        .after(add_frame_to_freshly_added_despawn_markers),
                )
                    .in_set(TimewarpSet::RecordComponentValues),
            )
            // ---
            // TimewarpSet::RollbackUnderwayComponents
            // * run_if(resource_exists(Rollback))
            // ---
            .add_systems(
                FixedUpdate,
                (
                    apply_snapshot_to_component_if_available::<T>,
                    rekill_components_during_rollback::<T>,
                    rebirth_components_during_rollback::<T>,
                    clear_removed_components_queue::<T>
                        .after(rekill_components_during_rollback::<T>),
                )
                    .in_set(TimewarpSet::RollbackUnderwayComponents),
            )
            // ---
            // TimewarpSet::RollbackInitiated
            // * run_if(resource_added(Rollback))
            // ---
            .add_systems(
                FixedUpdate,
                rollback_component::<T>
                    .after(rollback_initiated)
                    .in_set(TimewarpSet::RollbackInitiated),
            )
            // ---
            // TimewarpSet::NoRollback
            // * run_if(not(resource_exists(Rollback)))
            // ---
            .add_systems(
                FixedUpdate,
                (
                    record_component_death::<T>,
                    apply_snapshot_to_component_if_available::<T>,
                    trigger_rollback_when_snapshot_added::<T>,
                )
                    .before(consolidate_rollback_requests)
                    .in_set(TimewarpSet::NoRollback),
            )
    }
}
