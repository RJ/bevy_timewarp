use crate::systems::*;
use bevy::prelude::*;

use super::*;

/// This is an empty trait, used as a trait alias to make things more readable
/// see: https://www.worthe-it.co.za/blog/2017-01-15-aliasing-traits-in-rust.html
pub trait TimewarpComponent: Component + Clone + PartialEq + std::fmt::Debug
where
    Self: std::marker::Sized,
{
}

impl<T> TimewarpComponent for T
where
    T: Component + Clone + PartialEq + std::fmt::Debug,
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
    fn register_blueprint<T: TimewarpComponent>(&mut self) -> &mut Self;
}

impl TimewarpTraits for App {
    fn register_rollback<T: TimewarpComponent>(&mut self) -> &mut Self {
        self.register_rollback_with_options::<T, false>()
    }
    fn register_rollback_with_correction_logging<T: TimewarpComponent>(&mut self) -> &mut Self {
        self.register_rollback_with_options::<T, true>()
    }
    fn register_blueprint<T: TimewarpComponent>(&mut self) -> &mut Self {
        let config = self
            .world
            .get_resource::<TimewarpConfig>()
            .expect("TimewarpConfig resource expected");
        let schedule = config.schedule();
        // when we rollback, unpack anything wrapped up for this frame.
        // this handles the case where we are rolling back because of a wrapped blueprint, and
        // we hit the exact frame to unwrap it like this:
        self.add_systems(
            schedule.clone(),
            prefix_blueprints::unwrap_blueprints_at_target_frame::<T>
                .in_set(TimewarpPrefixSet::UnwrapBlueprints),
        );
        self.add_systems(
            schedule.clone(),
            prefix_check_if_rollback_needed::trigger_rollback_when_blueprint_added::<T>
                .before(prefix_check_if_rollback_needed::consolidate_rollback_requests)
                .in_set(TimewarpPrefixSet::CheckIfRollbackNeeded),
        )
    }
    fn register_rollback_with_options<T: TimewarpComponent, const CORRECTION_LOGGING: bool>(
        &mut self,
    ) -> &mut Self {
        let config = self
            .world
            .get_resource::<TimewarpConfig>()
            .expect("TimewarpConfig resource expected");
        let schedule = config.schedule();

        /*
               Prefix Systems
        */
        self.add_systems(
            schedule.clone(),
            (
                prefix_during_rollback::record_component_death::<T>,
                prefix_during_rollback::rebirth_components_during_rollback::<T>,
            )
                .in_set(TimewarpPrefixSet::DuringRollback),
        );
        self.add_systems(
            schedule.clone(),
            ((
                (
                    prefix_jit::apply_jit_icafs::<T, CORRECTION_LOGGING>,
                    prefix_jit::apply_jit_ss::<T>,
                ),
                apply_deferred,
            )
                .chain())
            .in_set(TimewarpPrefixSet::ApplyJustInTimeComponents),
        );
        // this may result in a Rollback resource being inserted.
        self.add_systems(
            schedule.clone(),
            (
                prefix_check_if_rollback_needed::detect_misuse_of_icaf::<T>,
                prefix_check_if_rollback_needed::trigger_rollback_when_icaf_added::<
                    T,
                    CORRECTION_LOGGING,
                >,
                prefix_check_if_rollback_needed::trigger_rollback_when_snapshot_added::<T>,
            )
                .before(prefix_check_if_rollback_needed::consolidate_rollback_requests)
                .in_set(TimewarpPrefixSet::CheckIfRollbackNeeded),
        );
        self.add_systems(
            schedule.clone(),
            prefix_start_rollback::rollback_component::<T>
                .in_set(TimewarpPrefixSet::StartRollback)
                .after(prefix_start_rollback::rollback_initiated),
        );

        /*
               Postfix Systems
        */
        self.add_systems(
            schedule.clone(),
            (
                postfix_components::remove_components_from_despawning_entities::<T>
                    .after(postfix_components::remove_descendents_from_despawning_entities),
                postfix_components::record_component_history::<T>,
                postfix_components::add_timewarp_components::<T, CORRECTION_LOGGING>,
                postfix_components::record_component_birth::<T>,
            )
                .in_set(TimewarpPostfixSet::Components),
        );
        self.add_systems(
            schedule.clone(),
            (
                postfix_during_rollback::rekill_components_during_rollback::<T>,
                postfix_during_rollback::clear_removed_components_queue::<T>,
            )
                .in_set(TimewarpPostfixSet::DuringRollback),
        )
    }
}
