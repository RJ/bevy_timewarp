## Integrating bevy_timewarp with bevy_replicon

I'm using [bevy_replicon](https://crates.io/crates/bevy_replicon) in my game, alongside bevy_timewarp.
You can use custom deserializers with replicon to write updates into the `ServerSnapshot` buffer.

### Custom timewarp deserializer example

This is for [bevy_xpbd](https://crates.io/crates/bevy_xpbd_2d)'s `Rotation` component:

```rust
pub fn deserialize_rotation_timewarp(
    entity: &mut EntityMut,
    _entity_map: &mut NetworkEntityMap,
    mut cursor: &mut Cursor<Bytes>,
    tick: RepliconTick,
) -> Result<(), bincode::Error> {
    let sin: f32 = bincode::deserialize_from(&mut cursor)?;
    let cos: f32 = bincode::deserialize_from(&mut cursor)?;
    let comp = Rotation::from_sin_cos(sin, cos);
    if let Some(mut ss) = entity.get_mut::<ServerSnapshot<Rotation>>() {
        ss.insert(tick.get(), comp);
    } else {
        entity.insert(comp);
    }
    Ok(())
}

pub fn serialize_rotation(
    component: Ptr,
    mut cursor: &mut Cursor<Vec<u8>>,
) -> Result<(), bincode::Error> {
    // SAFETY: Function called for registered `ComponentId`.
    let comp: &Rotation = unsafe { component.deref() };
    bincode::serialize_into(&mut cursor, &comp.sin())?;
    bincode::serialize_into(&mut cursor, &comp.cos())
}
```

### Fixed Timestep

You'll need to set replicon's TickPolicy to manual, and advance the replicon tick on the server in line with your tick counter in your fixed timestep.

```rust
    #[cfg(feature = "is_server")]
    {
        app.add_plugins(
            ReplicationPlugins
                .build()
                .disable::<ClientPlugin>()
                .set(ServerPlugin::new(TickPolicy::Manual)),
        );
        // we set the tick policy to manual, and want to do sending AFTER physics every fixed step,
        // so clients get the updated-by-physics-systems versions of pos, vel, etc.
        app.configure_set(
            FixedUpdate,
            ServerSet::Send.after(SpacepitSet::AfterPhysics),
        );
    }
```

And scheduled in your fixed timestep:

```rust
pub fn frame_inc_and_replicon_tick_sync(
    mut game_clock: ResMut<GameClock>, // your own tick counter
    mut replicon_tick: ResMut<bevy_replicon::prelude::RepliconTick>,
) {
    // advance your tick and replicon's tick in lockstep
    game_clock.advance(1);
    let delta = game_clock.frame().saturating_sub(replicon_tick.get());
    replicon_tick.increment_by(delta);
}
```

With this setup, the `RepliconTick` you receive in the deserialize functions will match up with your game's fixed timestep tick.
