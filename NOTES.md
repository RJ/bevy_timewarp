<style type="text/css">
table.schedules td:first-child {
    font-family: 'Courier New', monospace;
    font-size: 16px;
    font-weight: bold;
}

table.sets td:first-child {
    font-family: 'Courier New', monospace;
    font-size: 14px;
}
table.sets-server td:first-child {
    font-family: 'Courier New', monospace;
    font-size: 14px;
}


table.sets td:nth-child(2) {
    font-variant: small-caps;
    font-size: 13px;
}

tr.fixedupdate td:first-child {
    background-color: #f0f0f0;
}
</style>

# Anatomy of a Frame

## Server Frame

The server ticks along at the fixed rate, never rewinding or rolling back. Player inputs must arrive
in time for the frame they are associated with to be simulated.

The server runs the game simulation, applying forces to player objects in response to inputs,
then runs the physics update sets, then broadcasts the new position of objects.

<table class="schedules">
<tr>
<td>PreUpdate</td>
<td>Replicon reads from network, writes to <code>Events&lt;IncomingMessage&gt;</code></td>
</tr>
<tr class="fixedupdate">
<td>FixedUpdate</td>
<td>

<table class="sets-server">
<tr>
<th colspan="2">My Game Sets</th>
</tr>
        <tr>
            <td>First</td>
            <td>Increments tick</td>
        </tr>
        <tr>
            <td>IncomingMessages</td>
            <td>Reads and handles <code>Events&lt;IncomingMessage&gt;</code><br><small>eg, player 1's input for frame F = "Fire + Turn Left"</small></td>
        </tr>
        <tr>
            <td>GameSimulation</td>
            <td>Turns player inputs into forces, actions, spawns, etc</td>
        </tr>
        <tr>
            <td>OutgoingMessages</td>
            <td>Writes custom game events to network (ie not replicated component state)<br><small>eg, broadcasting chat messages, or player inputs, to all other players</small></td>
        </tr>
</table>


</td>
</tr>
<tr class="fixedupdate">
<td>FixedUpdate</td>
<td>

<table class="sets-server">
<tr>
<th colspan="3">bevy_xpbd</th>
</tr>
<tr>
<td>Physics</td>
<td>All the physics sets run here, controlled by bevy_xpbd</td>
</tr>
</table>

</td>
</tr>
<tr>
<td>PostUpdate</td>
<td>Replicon broadcasts entity/component changes (the post-physics values for this frame)</td>
</tr>
<tr>
<td>PostUpdate</td>
<td>Rendering (if server is built with "gui" feature)</td>
</tr>
</table>

## Client Frame

The client tries to be a few frames ahead of the server's simulation, such that inputs for
frame F arrive by frame F-1 on the server.

This means inputs from the server arriving at the client are, from the client's POV, in the past.

<table class="schedules">

<tr>
<td>PreUpdate</td>
<td>
Replicon reads from network, deserializes and applies the replication data to the client. 
This can include spawning new entities and updating components.
For timewarp-specific components, the new updates are written to the <code>ServerSnapshot&lt;Component&gt;</code> at
the <code>RepliconTick</code>. Timewarp applies the updates to the components at the correct frame during rollback, as required.
<br>
New custom events are written to the bevy event queue, to be consumed by my game's systems.
</td>
</tr>

<tr class="fixedupdate">
<td>FixedUpdate</td>
<td>

<table class="sets">
<tr>
<th colspan="3">Timewarp Prefix Sets</th>
</tr>
<tr>
<td>First</td>
<td>always</td>
<td>sanity check systems prevent blowing your own foot off, kinda.</td>
</tr>
<tr>
<td>InRollback</td>
<td>in rb</td>
<td>During rollback, we check if we should exit rollback, having resimulated everything in the requested rollback range.</td>
</tr>
<tr>
<td>NotInRollback</td>
<td>not in rb</td>
<td>Check if a newly added ServerSnapshot or ABAF/ICAF means we request a rollback</td>
</tr>
<tr>
<td>StartRollback</td>
<td>new rb</td>
<td>
If previous set requested a rollback, we wind back the game clock, and load in component data for
the appropriate frame for starting rollback.
</td>
</tr>
<tr>
<td>UnwrapBlueprints</td>
<td>always</td>
<td>Unwrap ABAFs for this frame</td>
</tr>
<tr>
<td>Last</td>
<td>always</td>
<td>...</td>
</tr>
</table>

</td>
</tr>

<tr class="fixedupdate">
<td>FixedUpdate</td>
<td>

<table class="sets">
<tr>
    <th colspan="3">My Game Sets</th>
</tr>
<tr>
    <td>First</td>
    <td>always</td>
    <td>Increment GameClock `gc.frame += 1`  </td>
</tr>

<tr>
    <td>IncomingMessages</td>
    <td>not rb</td>
    <td>
    Reads and handles <code>Events&lt;IncomingMessage&gt;</code>  
    </td>
</tr>
<tr>
    <td>Housekeeping</td>
    <td>not rb</td>
    <td>
    Monitoring lag, tuning various things, collecting metrics  
    </td>
</tr>
<tr>
    <td>GameSimulation</td>
    <td>always</td>
    <td>
   Apply player inputs to simulation. Fetches inputs for game_clock.current_frame() from storage,
   so will apply correct inputs during rollback.
    </td>
</tr>
<tr>
    <td>AssembleBlueprints</td>
    <td>always</td>
    <td>
   Any new blueprint components get assembled (ie, bunch of components get added)
    </td>
</tr>
<tr>
    <td>OutgoingMessages</td>
    <td>not rb</td>
    <td>
    Writes custom game events to network<br><small>Send our inputs for this frame to the server</small>
    </td>
</tr>
</table>



</td>
</tr>
<tr class="fixedupdate">
<td>FixedUpdate</td>
<td>


<table class="sets">
<tr>
<th colspan="3">bevy_xpbd</th>
</tr>
<tr>
<td>Physics</td>
<td>always</td>
<td>All the physics sets run here, controlled by bevy_xpbd</td>
</tr>
</table>

</td>
</tr>
<tr class="fixedupdate">
<td>FixedUpdate</td>
<td>
<table class="sets">
<tr>
<th colspan="3">Timewarp Postfix Sets</th>
</tr>
<tr>
<td>First</td>
<td>always</td>
<td>...</td>
</tr>
<tr>
<td>Components</td>
<td>always</td>
<td>
record component history to ComponentHistory(Component), clean up despawn requests, add
timewarp components to entities with freshly added tw-registered components.
record component births.
</td>
</tr>
<tr>
<td>InRollback</td>
<td>in rb</td>
<td>
wipe removed component queue, remove components which shouldn't exist at this frame
</td>
</tr>
<tr>
<td>Last</td>
<td>always</td>
<td>...</td>
</tr>
</table>

</td>
</tr>
<tr>
<td>PostUpdate</td>
<td>Messages "sent" in OutgoingMessages are sent now by Replicon.</td>
</tr>
<tr>
<td>PostUpdate</td>
<td>Rendering</td>
</tr>
</table>


## How rollbacks happen

Systems that initiate rollbacks write a `RollbackRequest` to an event queue, specifying the frame they
wish to start resimulating from. These are in the `NotInRollback` set.

All rollback requests are consolidated, and a `Rollback` resource is written. The `RollbackConsolidationStrategy` from `TimewarpConfig`
determines if the oldest or newest frame from the list of requests is used.
If you only receive entire-world updates at a time, taking the newest frame requested is optimal. 
This is how replicon currently works, and is the default.

If we need to resimulate from frame `N` onwards, before we start simulating that frame, we load in
stored component values from frame `N - 1`. 

We also unwrap any blueprints (ABAF) for frame `N`.

## On blueprint and component temporality

The server sends replicon data containing component values in PostUpdate, after physics.

So when the client receives a packet saying that a component value is X at frame `N`, that means
the value was X on the server, after frame `N` was simulated.

So if the client receives this, they can resimulate from frame `N+1`, and set the component to X
before starting - representing the correct state at the end of frame `N`.

### Spawning via blueprint

Say the server spawns a new player during frame 100. It inserts a PlayerShip blueprint, and then
the blueprint assembly fn for players adds a Position, Collider, etc.
At the end of the frame in postupdate, replicon sends this data out to clients.

That player entity might have been given a position of X,Y during server's frame 100, but during
physics that position might have changed to X',Y' before replication data was sent.

On clients, when we get a player blueprint for frame 100 we'll be rolling back to 101,
and inserting the replicated Pos value@100 before we start simulating 101.
But we'll only end up adding the (non-replicated) collider during bp assembly in frame 101.
Although not optimal, this is correct – the replicated components are correct for that frame.

Anything the player's collider interacted with during physics on frame 100 on the server may end up
requiring a correction on the client (since client couldn't have simualted that). but that's fine.

if we embed the pos/whatever into the BP we could actually assemble on the correct frame?

REALLY, the actual bundles on the server are what we need to replicate, like we do for BPs at the mo.
so the current blueprint comps need all the stuff that's in the bundle, then the assembly fn creates the comps.
then we can rollback and insert blueprints on the same frame the server did - even though replicon will have
also received the post-physics replicated data for them at the end of that frame. our bp assembly fn should overwrite 
it on the client. 

HOWEVER then we get stale data left over in the blueprint (like the pos from when it was spawned) and
we can't really filter or control how replicon sends these bp compoents to clients.
we don't want to continually update the BPs copying in fresh data, just so new players can spawn
stuff from existing blueprints. kinda want a way to dynamically create a blueprint-bundle just for
replicating to new clients.

that's annoying. 

that's why we spawn blueprints the next frame on the client.

This isn't a concern for bullets that get predicted and spammed, so it's ok 🥺

<hr>

| __WARNING__ unedited ravings below this line that haven't yet necessarily coalesced into useful code

<hr>

## Client INPUT DELAY means other players' inputs can arrive early

As a client it's possible to get a player's input for a future frame before getting the component data, ie
before you simulates that frame.

A low lag player with a 3 frame input delay will send inputs for frame 103 to the server while the server is doing frame 101, and the server will rebroadcast, so you might get them a frame before needed on a remote client.

so perhaps we should be locally simulating the bullet spawn for remote players too somehow?

when the server gets a fire command, before broadcasting it to others, it should immediately spawn 
an entity, even if input is for a future frame. associate the spawned entity with the input command on the server, and
replicate the entity with a FutureBullet(f=100, client_id=X) component.

it's possible the client coule receive this right before they even simulate f100. in which case they can assemble on the correct frame, set the pos
using the same local prediction logic as firing yourself (because we wouldn't have comps from the server's assembly of the bp yet), and wait on normal server updates to arrive to correct it.

when the server does the bp assembly, it will have the prespawned entity associated with the input, and it just
assembles into that entity.

Maybe all firing should work this way, rather than apply_inputs doing it? 
apply inputs makes sense for thrusting and rotating etc, but when you need a new entity it's different.

FireIntent<Bullet>(client_id, frame, bullet_blueprint) component ? could do the same on the client, since the client also prespawns
entities to send them to the server, for matching up. can add the Prediction comp to that too to clean up misfires.

## FireIntent

>intent to fire is locked in at the time of sampling inputs for a future frame (input delay..), and immediately sent to the server.

local client A, simulating f100 with input delay of 3. so it's sampling inputs for 103 at f100.
presses fire. spawns an entity with Predicted component with entity id of af_100_103
sends fire input to server, with associated entity of af_100_103.

server receives the input in time for frame 101. 
server spawns an entity sf_101_103_a with Replicated and FireIntent<Bullet>(bullet_bp, f=103, client_id=A).
(this entity doesnt have physics components, or even a transform yet. invisible.)
server adds mapping for client A between af_100_103 <--> sf_101_103_a, which is sent back to A next packet.
server replicates entity sf_101_103_a to all players.

client B, about to simulate frame 103, (so in prefix clock still 102) receives the sf_101_103_a entity.
server has not yet delivered replication data for 103 to this client, so no pos@100 for the new bullet.
notices the FireIntent with a f=103, and unwraps it into a normal Bullet blueprint component.
client B simulates f 103, assembles bullet using normal prediction logic, into the fireintent entity.
will receive updates to it normally since it's already a server entity.

client C, on a very low lag connection, is about to simulate frame 102, (so in prefix clock still 101) receives the sf_101_103_a entity.
the entity gets created on the client with the fireintent, but not unwrapped, because fireintent.frame=103, and next client frame will be 102.
...
NEXT frame, the client unwraps it at frame=103. all good, as per the B client.

client D, on a high lag connection, receives the sf_101_103_a entity as it's about to simulate f 108.
issues RollbackRequest(104)
in prefix, while clock was wound back to 103, the fireintent bp is 





















