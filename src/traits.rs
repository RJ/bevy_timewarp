use crate::systems::*;
use bevy::{ecs::world::EntityMut, prelude::*};

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
            //  this apply_deferred is a hack so Res<Rollback> is visible for debugging in this systeem
            (
                apply_deferred,
                prefix_blueprints::unwrap_blueprints_at_target_frame::<T>,
            )
                .in_set(TimewarpPrefixSet::UnwrapBlueprints),
        );
        self.add_systems(
            schedule.clone(),
            prefix_not_in_rollback::request_rollback_for_blueprints::<T>
                .before(prefix_not_in_rollback::consolidate_rollback_requests)
                .in_set(TimewarpPrefixSet::NotInRollback),
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
        if CORRECTION_LOGGING {
            self.add_systems(
                schedule.clone(),
                prefix_first::enable_error_correction_for_new_component_histories::<T>
                    .in_set(TimewarpPrefixSet::First),
            );
        }
        self.add_systems(
            schedule.clone(), // TODO RJRJR move to _first file?
            prefix_not_in_rollback::detect_misuse_of_icaf::<T>.in_set(TimewarpPrefixSet::First),
        );
        self.add_systems(
            schedule.clone(), // TODO RJRJ MOVE FILE
            prefix_first::record_component_death::<T>
                .run_if(not(resource_exists::<Rollback>()))
                .in_set(TimewarpPrefixSet::First),
        );
        self.add_systems(
            schedule.clone(),
            (prefix_in_rollback::rebirth_components_during_rollback::<T>,)
                .in_set(TimewarpPrefixSet::InRollback),
        );
        // this may result in a Rollback resource being inserted.
        self.add_systems(
            schedule.clone(),
            (
                prefix_not_in_rollback::detect_misuse_of_icaf::<T>,
                prefix_not_in_rollback::unpack_icafs_and_maybe_rollback::<T, CORRECTION_LOGGING>,
                prefix_not_in_rollback::apply_snapshots_and_maybe_rollback::<T>,
            )
                .before(prefix_not_in_rollback::consolidate_rollback_requests)
                .in_set(TimewarpPrefixSet::NotInRollback),
        );
        self.add_systems(
            schedule.clone(),
            (prefix_start_rollback::rollback_component::<T>,)
                .in_set(TimewarpPrefixSet::StartRollback)
                .after(prefix_start_rollback::rollback_initiated),
        );

        /*
               Postfix Systems
        */
        self.add_systems(
            schedule.clone(),
            (
                postfix_components::remove_components_from_despawning_entities::<T>,
                postfix_components::record_component_history::<T>,
                postfix_components::add_timewarp_components::<T, CORRECTION_LOGGING>,
                postfix_components::record_component_birth::<T>,
            )
                .in_set(TimewarpPostfixSet::Components),
        );
        self.add_systems(
            schedule.clone(),
            (
                postfix_in_rollback::rekill_components_during_rollback::<T>,
                postfix_in_rollback::clear_removed_components_queue::<T>,
            )
                .in_set(TimewarpPostfixSet::InRollback),
        )
    }
}

pub enum InsertComponentResult {
    /// means the SS already existed
    IntoExistingSnapshot,
    /// had to add the timewarp components. SS, CH.
    ComponentsAdded,
}

/// This exists to make my replicon custom deserializing functions nicer.
/// in theory you can do this with checks for SS or InsertComponentAtFrame everywhere.
pub trait TimewarpEntityMutTraits {
    /// For inserting a component into a specific frame.
    /// Timewarp systems will insert into the entity at the correct point.
    fn insert_component_at_frame<T: TimewarpComponent>(
        &mut self,
        frame: FrameNumber,
        component: &T,
    ) -> Result<InsertComponentResult, TimewarpError>;
}

impl TimewarpEntityMutTraits for EntityMut<'_> {
    fn insert_component_at_frame<T: TimewarpComponent>(
        &mut self,
        frame: FrameNumber,
        component: &T,
    ) -> Result<InsertComponentResult, TimewarpError> {
        if let Some(mut ss) = self.get_mut::<ServerSnapshot<T>>() {
            ss.insert(frame, component.clone())?;
            Ok(InsertComponentResult::IntoExistingSnapshot)
        } else {
            let tw_config = self
                .world()
                .get_resource::<TimewarpConfig>()
                .expect("TimewarpConfig resource missing");
            let window_size = tw_config.rollback_window() as usize;
            // insert component value at this frame, since the system that records it won't run
            // if a rollback is happening this frame. and if it does it just overwrites
            let comp_history = ComponentHistory::<T>::with_capacity(
                // timewarp_config.rollback_window as usize,
                window_size,
                frame,
                component.clone(),
                &self.id(),
            );

            let mut ss = ServerSnapshot::<T>::with_capacity(window_size * 60);
            ss.insert(frame, component.clone())
                .expect("fresh one can't fail");
            // (tw system sets correction logging for us later, if needed)
            debug!(
                "Adding SS/CH to {:?} for {}\nInitial val @ {:?} = {:?}",
                self.id(),
                std::any::type_name::<T>(),
                frame,
                component.clone(),
            );

            self.insert((comp_history, ss, TimewarpStatus::new(frame)));
            Ok(InsertComponentResult::ComponentsAdded)
        }
    }
}
