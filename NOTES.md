# Anatomy of a Frame

## Server Frame

The server ticks along at the fixed rate, never rewinding or rolling back. Player inputs must arrive
in time for the frame they are associated with to be simulated.

The server runs the game simulation, applying forces to player objects in response to inputs,
then runs the physics update sets, then broadcasts the new position of objects.

<table>
<tr>
<th>Schedule</th><th> Notes</th>
</tr>
<tr>
<td>PreUpdate</td>
<td>Replicon reads from network, writes to <code>Events&lt;IncomingMessage&gt;</code></td>
</tr>
<tr style="background-color:#f0f0f0">
<td>FixedUpdate</td>
<td><strong>Main Game Simulation</strong><br>

| Set              | Action                                                                                                                                                            |
| ---------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| First            | Increment tick                                                                                                                                                    |
| IncomingMessages | Reads and handles <code>Events&lt;IncomingMessage&gt;</code><br><small>eg, player 1's input for frame F = "Fire + Turn Left"</small>                              |
| GameSimulation   | Turns player inputs into forces, actions, spawns, etc                                                                                                             |
| OutgoingMessages | Writes custom game events to network (ie not replicated component state)<br><small>eg, broadcasting chat messages, or player inputs, to all other players</small> |

</td>
</tr>
<tr style="background-color:#f0f0f0">
<td>FixedUpdate</td>
<td>Physics Sets Run Here (bevy_xpbd)</td>
</tr>
<tr>
<td>PostUpdate</td>
<td>Replicon broadcasts entity/component changes (the post-physics values for this frame)</td>
</tr>
</table>

## Client

The client tries to be a few frames ahead of the server's simulation, such that inputs for
frame F arrive by frame F-1 on the server.

This means inputs from the server arriving at the client are, from the client's POV, in the past.

<table>
<tr>
<th>Schedule</th><th> Notes</th>
</tr>
<tr>
<td>PreUpdate</td>
<td>Replicon reads from network, writes to <code>Events&lt;IncomingMessage&gt;</code></td>
</tr>
<tr style="background-color:#f0f0f0">
<td>FixedUpdate</td>
<td><strong>Timewarp Prefix</strong><br>

| Set                   | Action                                                                                                                                                                 |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| CheckIfRollbackNeeded | Was a SS updated, or icaf added, with a new value for `ss.frame < gc.frame`? if so, trigger rollback                                                                   |
| ApplyComponents      | Apply anything for frame `gc.frame` - we're about to increment, and then need last frame's values to exist. This could be values in SS, or ICAF components to unwrap. |

</td>
</tr>
<tr style="background-color:#f0f0f0">
<td>FixedUpdate</td>
<td><strong>Main Game Simulation</strong><br>

| Set              | Action                                                                                              |
| ---------------- | --------------------------------------------------------------------------------------------------- |
| First            | Increment GameClock `gc.frame += 1`                                                                 |
| IncomingMessages | Reads and handles <code>Events&lt;IncomingMessage&gt;</code>                                        |
| Housekeeping     | Monitoring lag, tuning various things, collecting metrics                                           |
| GameSimulation   | Apply player inputs to simulation                                                                   |
| OutgoingMessages | Writes custom game events to network<br><small>Send our inputs for this frame to the server</small> |

</td>
</tr>
<tr style="background-color:#f0f0f0">
<td>FixedUpdate</td>
<td>Physics Sets Run Here (bevy_xpbd)</td>
</tr>
<tr style="background-color:#f0f0f0">
<td>FixedUpdate</td>
<td><strong>Timewarp Postfix</strong><br>

| Set                        | Action                                                    |
| -------------------------- | --------------------------------------------------------- |
| RollbackStartMarker        | remove components from despawning entities                |
| RecordComponentValues      | write to component history, record births and deaths, etc |
| RollbackUnderwayComponents | apply stored values to actual components                  |
| RolbackUnderwayGlobal      |                                                           |
| NoRollback                 |                                                           |
| RollbackEndMarker          |                                                           |

</td>
</tr>
<tr>
<td>PostUpdate</td>
<td>Replicon broadcasts entity/component changes (post-physics values for this frame)</td>
</tr>
</table>

## OOh

recording death of component = write 30 Nones to following frames in CH, covering the rollback period
or just assume no CH value = remove at frame.

## Deserializing a timewarp component

The process for deserializing timewarp-registered components is as follows.

In `PreUpdate` on the client, when it receives a component update, that update is for the included
`RepliconTick`. That is the server's post-physics value for that frame (note the server broadcasts replication data AFTER the physics sets).

Also note, in the client's `PreUpdate` the frame number (`GameClock.frame()`) represents the previous FixedUpdate to run,
and will be incremented in the First set in FixedUpdate next time it runs. In other words, the client has already simulated the frame number seen in PreUpdate.

### A Packet Arrives for RepliconTick 100

The deserialize function needs to tell timewarp we have authoritative data for RepliconTick 100.
It checks if the associated entity for the compoent has a `ServerSnapshot<Position>` – if so, it does
`ss.insert(100, comp_data)`, otherwise it inserts a `InsertComponentAtFrame(100, comp_data)` component.

<hr>


In `PreUpdate` a `Position` component replication update is deserialized. The `RepliconTick` is 100. The `Entity` for the component is specified, but it might have just been spawned by replicon in response to a server spawn, or perhaps it's an existing entity.

Since this is a post-physics value at frame 100, we don't want to see it at the start of our FixedUpdate unless our frame number is 101.

The client is supposed to be ahead of the server.

but let's say the client's preup.f is 100, about to start frame 101. The client is somehow not ahead of the server enough.
We need to see it at the start of fixed frame 101, and we can't wait for TW sets after our loop to unpack it. 
We either ICAF or SS.insert it, and rely on TW.prefix to unpack immediately at the start of our loop.
this shouldn't trigger a rollback. (or we insert directly in the deser? then we lose info that it's server-authoritative tho)


what if client's preup.f is 101, about to simulate 102?
we needed it to exist at the start of the last frame, so we'll need to rollback.
SS.insert or ICAF - this should trigger a rollback by settings the clock to 100,
then enting the start of fixedupdate, which should apply values from 100 to components.
then we increment to 101 as normal.




### Does the entity already have ServerSnapshot&lt;Position&gt;?
If so, it's not a freshly spawned entity.
Write to the SS: `ServerSnapshot<Position>.insert(packet.replicon_tick, packet.component_value)`


### No SS

Insert a timewarp component: `InsertComponentAtFrame(packet.replicon_frame, packet.component_value)`


## Deserializing a blueprint component

The process for deserailzing a blueprint component – one which is not subject to rollback by timewarp,
but might trigger creation of timewarp-registered components in the blueprint factory function – is as follows.













