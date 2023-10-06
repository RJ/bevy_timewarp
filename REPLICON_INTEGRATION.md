## Integrating bevy_timewarp with bevy_replicon

I'm using [bevy_replicon](https://crates.io/crates/bevy_replicon) in my game, alongside bevy_timewarp.
You can use custom deserializers with replicon to write updates into the `ServerSnapshot` buffer like this:

### Custom timewarp deserializer example

This is for [bevy_xpbd](https://crates.io/crates/bevy_xpbd_2d)'s `Rotation`` component:

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
    if let Some(mut sync_status) = entity.get_mut::<SyncStatus>() {
        sync_status.last_snapshot_rot = Some(comp);
        sync_status.last_update_frame = tick.get();
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
