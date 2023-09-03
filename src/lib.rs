//! Buffer and rollback to states up to a few frames ago, for rollback networking.
//!
//! Doesn't do any networking, just concerned with buffering old component states in order to
//! revert values to previous frames and quickly fast-forward back to the original frame.
//! Assumes game logic uses bevy's `FixedUpdate` schedule.
//!
//! Current status: under development alongside a multiplayer server-authoritative game.
//! I am "RJ" on bevy's discord, in the networking channel, if you want to discuss.
//!
//! ## "Sports Games"
//!
//! This crate is built for so called "sports games" where dynamic entities interact in the same
//! timeframe. That means clients simulate all entities into the future, in the same timeframe as
//! the local player. This necessitates predicting player inputs.
//!
//! This is quite different to traditional Quake style FPS netcode, which is for a mostly static
//! world, with players running around shooting eachother. In Quake style, you are always seeing
//! a delayed version of other players, interpolated between two snapshots. The server does
//! backwards reconcilliation / lag compensation to verify hits.
//!
//! ### Example rollback scenario:
//!
//! In your client/server game:
//!
//! - client is simulating frame 10
//! - server snapshot for frame 6 arrives, including values for an entity's component T
//! - client updates entity's ServerSnapshot<T> value at frame 6 (ie, the past)
//! - Timewarp triggers a rollback to frame 6 if snapshot != our stored value for frame 6.
//! - - winds back frame counter to 6
//!   - copies the server snapshot value to the component
//!   - resimulates frames 7,8,9,10 as fast as possible
//!   - during this process, your systems would apply player inputs you've stored for each frame
//! - Rollback ends and frame 11 continues normally.
//!
//! ## Quickstart
//!
//! Add the plugin, then register the components you wish to be rollback capable:
//!
//! ```rust,ignore
//! // Store 10 frames of rollback, run timewarp systems after our GameLogic set.
//! app.add_plugins(TimewarpPlugin::new(10, MySets::GameLogic));
//! // register components that should be buffered and rolled back as needed:
//! app.register_rollback::<MyComponent>();
//! app.register_rollback::<Position>();
//! // etc..
//! ```
//!
//! Any entity that has a `T` Component will automatically be given a [`ComponentHistory<T>`] and
//! [`ServerSnapshot<T>`] component.
//!
//! `ComponentHistory` is a circular buffer of the last N frames of component values.
//! This is logged every frame automatically, so is mostly your client predicted values.
//! You typically won't need to interact with this.
//!
//! `ServerSnapshot` is a buffer of the last few authoritative component values, typically what
//! you received from the game server. Your network system will need to add new values to this.
//!
//! When you receive authoritative updates, add them to the ServerSnapshot<MyComponent> like so:
//!
//! ```rust,ignore
//! fn process_position_updates_from_server(
//!     mut q: Query<(Entity, &mut ServerSnapshot<Position>)>,
//!     update: Res<UpdateFromServer>, // however you get your updates, not our business.
//! ){
//!     // typically update.frame would be in the past compared to current client frame
//!     for (entity, mut ss_position) in q.iter_mut() {
//!         let component_val = update.get_position_at_frame_for_entity(update.frame, entity); // whatever
//!         ss_position.insert(update.frame, component_val);
//!     }
//! }
//!```
//!
//! Alternatively, and especially if you are inserting a component your entity has never had before,
//! meaning there will be no `ServerSnapshot<T>` component, insert components in the past like this:
//!
//! ```rust,ignore
//! let add_at_frame = 123;
//! let historical_component = InsertComponentAtFrame::new(add_at_frame, MyComponent);
//! commands.entity(e1).insert(historical_component);
//! ```
//!
//! ### Systems configuration
//!
//! Divide up your game systems so that during a rollback you still apply stored player input,
//! but ignore stuff like sending network messages etc.
//!
//! During a rollback, the [`Rollback`] resource will exist. Use this in a `run_if` condition.
//!
//! ```rust,ignore
//! // Normal game loop when not doing a rollback/fast-forward
//! app.add_systems(FixedUpdate,
//!     (
//!         frame_inc,
//!         process_server_messages,
//!         process_position_updates_from_server,
//!         spawn_stuff,
//!         read_player_inputs,
//!         apply_all_player_inputs_to_simulation_for_this_frame,
//!         do_physics,
//!         render,
//!         etc,
//!     )
//!     .chain()
//!     .in_set(MySets::GameLogic)
//!     .run_if(not(resource_exists::<Rollback>())) // NOT in rollback
//! );
//! // Abridged game loop for replaying frames during rollback/fast-forward
//! app.add_systems(FixedUpdate,
//!     (
//!         frame_inc,
//!         apply_all_player_inputs_to_simulation_for_this_frame,
//!         do_physics,
//!     )
//!     .chain()
//!     .in_set(MySets::GameLogic)
//!     .run_if(resource_exists::<Rollback>()) // Rollback is happening.
//! );
//! ```
//!
//! # Visual smoothing of errors
//!
//! Timewarp snaps the simulation state – ie. the value of a component at a specific frame simulated
//! locally vs the value after rollback and resimulate might differ.
//! If you register your components like this:
//!
//! ```rust,ignore
//!     app.register_rollback_with_correction_logging::<Position>();
//! ```
//!
//! then timewarp will capture the before and after versions of components when doing a rollback,
//! and put it into a [`TimewarpCorrection<Position>`] component for your game to examine.
//! Typically this would be useful for some visual smoothing - you might gradually blend over the
//! error distance with your sprite, even though the underlying physical simulation snapped correct.
//!
//! ## Testing various edge cases
//!
//! TODO: I don't know how to link rustdocs to integration tests..
//!
//! See the `basic_rollback` test for the most
//! straightforward scenario: a long lived entity with components that haven't been added/removed,
//! and an authoritative server update arrives. Apply value to past frame, rollback and continue.
//!
//! The `despawn_markers` test illustrates how to despawn –
//! rather than doing `commands.entity(id).despawn()`, which would remove all trace and thus
//! mean the entity couldn't be revived if we needed to rollback to a frame when it was alive,
//! you insert a [`DespawnMarker(frame_number)`][DespawnMarker] component, which cleans up
//! the entity immediately by removing all its registered components, then does the actual despawn
//! after `rollback_window` frames have elapsed.
//!
//! The `despawn_revival_during_rollback` test
//! does something similar, but triggers a rollback which will restore components to an entity
//! tagged with a `DespawnMarker` in order to resimulate after a server update arrives, and then
//! remove the components again.
//!
//! The `component_add_and_remove` test
//! tests how a server can add a component to an entity in the past, in this case a Shield, which
//! prevents our enemy from taking damage.
//!
//! TODO describe the various other tests.
//!
//! ## Quick explanation of entity death in our rollback world
//!
//! In order to preserve the `Entity` id, removing an entity using the `DespawnMarker` in fact
//! removes all its registered components, but leaves the bare entity alive for rollback_frames.
//!
//! Such entities retain their ComponentHistory buffers so they can be revived if needed because of
//! a rollback. Finally, after rollback_frames has elapsed, they are despawn_recursived.
//!
//! Removing components is hopefully a sufficient substitute for immediately despawning, however
//! be aware the entity id will still exist until finally despawned.
//!
//! ## Caveats:
//!
//! - Developing this alongside a simple game, so this is based on what I need for my attempt at
//!   a server-authoritative multiplayer game.
//! - Currently requires you to use [`GameClock`] struct from this crate as frame counter.
//! - Littered with a variety of debug logging, set your log level accordingly
//! - Unoptimized: clones components each frame without checking if they've changed.
//! - Doesn't rollback resources or other things, just (registered) component data.
//! - Registered components must impl `PartialEq`
//! - I'm using a patched version of `bevy_xpbd` at the mo, to make `Collider` impl `PartialEq`
//!   (PRs sent..)
//!
pub(crate) mod components;
mod error;
mod frame_buffer;
mod game_clock;
pub(crate) mod resources;
pub(crate) mod systems;
mod traits;

pub mod prelude {
    pub use crate::components::*;
    pub use crate::error::*;
    pub use crate::frame_buffer::*;
    pub use crate::game_clock::*;
    pub use crate::resources::*;
    pub use crate::traits::*;
    pub use crate::TimewarpPlugin;
    pub type FrameNumber = u32;
    pub use crate::systems::RollbackRequest;
    pub use crate::TimewarpSet;
}

use bevy::{ecs::schedule::BoxedSystemSet, prelude::*};
use prelude::*;

/// bevy_timewarp's systems run in these three sets, which get configured to run
/// after the main game logic (the set for which is provided in the plugin setup)
#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimewarpSet {
    RecordComponentValues,
    RollbackUnderwayComponents,
    RollbackUnderwayGlobal,
    RollbackInitiated,
    NoRollback,
}

#[derive(SystemSet, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TimewarpSetMarkers {
    /// remains empty - exists as marker for interleaving apply_deferreds
    RollbackStartMarker,
    /// remains empty - exists as marker for interleaving apply_deferreds
    RollbackEndMarker,
}

pub struct TimewarpPlugin {
    config: TimewarpConfig,
    /// set containing game logic, after which the rollback systems will run
    after_set: BoxedSystemSet,
}

impl TimewarpPlugin {
    pub fn new(rollback_window: FrameNumber, after_set: impl SystemSet) -> Self {
        Self {
            config: TimewarpConfig {
                rollback_window,
                ..default()
            },
            after_set: Box::new(after_set),
        }
    }
}

impl Plugin for TimewarpPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(self.config)
            // RollbackRequest events are drained manually in `consolidate_rollback_requests`
            .init_resource::<Events<RollbackRequest>>()
            .insert_resource(RollbackStats::default())
            .configure_sets(
                FixedUpdate,
                (
                    TimewarpSetMarkers::RollbackStartMarker,
                    // --- APPLY_DEFERRED ---
                    // Runs in normal frames       : Yes
                    // Runs when rollback initiated: Yes
                    // Runs during ongoing rollback: Yes
                    TimewarpSet::RecordComponentValues,
                    // --- APPLY_DEFERRED ---
                    // Runs in normal frames       : No
                    // Runs when rollback initiated: Yes
                    // Runs during ongoing rollback: Yes
                    TimewarpSet::RollbackUnderwayComponents.run_if(resource_exists::<Rollback>()),
                    // Runs in normal frames       : No
                    // Runs when rollback initiated: Yes
                    // Runs during ongoing rollback: Yes
                    TimewarpSet::RollbackUnderwayGlobal.run_if(resource_exists::<Rollback>()),
                    // Runs in normal frames       : Yes
                    // Runs when rollback initiated: No
                    // Runs during ongoing rollback: No
                    // systems in here can potentially initiate a rollback:
                    TimewarpSet::NoRollback.run_if(not(resource_exists::<Rollback>())),
                    // --- APPLY_DEFERRED ---
                    // Runs in normal frames       : No
                    // Runs when rollback initiated: Yes
                    // Runs during ongoing rollback: No
                    TimewarpSet::RollbackInitiated.run_if(resource_added::<Rollback>()),
                    TimewarpSetMarkers::RollbackEndMarker,
                )
                    .chain(),
            )
            // BoxedSystemSet implements IntoSystemSetConfig, but not IntoSystemSetConfigs
            // so for now, have to use the deprecated configure_set instead of configure_sets.
            // assume this is because bevy's API is in transitional phase in this regards..
            // https://github.com/bevyengine/bevy/pull/9247
            .configure_set(
                FixedUpdate,
                self.after_set
                    .dyn_clone()
                    .before(TimewarpSetMarkers::RollbackStartMarker),
            )
            .insert_resource(FixedTime::new_from_secs(1.0 / 60.0))
            .insert_resource(GameClock::new())
            // apply_deferred between rollback sets
            // needed because rollback logic can insert/remove components and the Rollback resource.
            .add_systems(
                FixedUpdate,
                (
                    apply_deferred
                        .after(TimewarpSetMarkers::RollbackStartMarker)
                        .before(TimewarpSet::RecordComponentValues),
                    apply_deferred
                        .after(TimewarpSet::RecordComponentValues)
                        .before(TimewarpSet::RollbackInitiated),
                    apply_deferred
                        .after(TimewarpSet::NoRollback)
                        .before(TimewarpSet::RollbackInitiated),
                ),
            )
            .add_systems(
                FixedUpdate,
                systems::add_frame_to_freshly_added_despawn_markers
                    .in_set(TimewarpSet::RecordComponentValues),
            )
            .add_systems(
                FixedUpdate,
                systems::check_for_rollback_completion.in_set(TimewarpSet::RollbackUnderwayGlobal),
            )
            .add_systems(
                FixedUpdate,
                systems::rollback_initiated.in_set(TimewarpSet::RollbackInitiated),
            )
            .add_systems(
                FixedUpdate,
                (
                    systems::consolidate_rollback_requests,
                    systems::despawn_entities_with_elapsed_despawn_marker,
                )
                    .chain()
                    .in_set(TimewarpSet::NoRollback),
            );
    }
}
