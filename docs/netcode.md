# Netcode

How a client and the dedicated server keep one game: the tick, the wire
format, prediction, interpolation, lag compensation, what it costs on the
wire, and what the server checks. Running and joining servers is in
[multiplayer.md](multiplayer.md).

The engine-free core is `rust/sim/src/net/`, so the client, the dedicated
server, and unit tests share it. The Godot side is `rust/game/src/net/`:
`NetServer` (`server.rs`), `NetClient` (`client.rs`), and `RemotePlayer`
(`remote.rs`).

## Model

Every match runs on a dedicated server, even a local match against bots. The
server is the authority: it moves every player from their inputs, runs their
weapons, resolves every shot, and keeps score. Clients predict their own
player and draw everyone else a little in the past.

| | Value | Where |
| --- | --- | --- |
| Simulation tick, server and client prediction | 120 Hz | `shooter_sim::TICK_RATE` |
| Snapshots, server to each client | 60 Hz (every 2 ticks) | `NetServer.snapshot_interval` |
| Input packets, client to server | 120 Hz, one a tick | `NetClient` |
| Players per server | 12 (6v6), plus 8 spectators | `net::MAX_PLAYERS`, `MAX_SPECTATORS` in `server.rs` |
| Protocol version | 93 | `protocol::PROTOCOL_VERSION` |
| Transport | Godot's ENet, two channels | `rust/game/src/net/transport.rs` |

ENet is only a packet pipe. Channel 0 carries snapshots and inputs, unreliable
and unsequenced; channel 1 carries the handshake and game events, reliable.
Every byte in a packet is written by `shooter_sim::net::protocol`, so the wire
format is tested without the engine.

## Wire protocol

`rust/sim/src/net/protocol.rs` holds every message, bit-packed
(`net::bits`).

- **Client to server.** A hello (protocol version, name, account session
  token, and whether to spectate), then one `InputPacket` a tick. Each packet carries every input the server has not acknowledged, up
  to 24 (200 ms), so a lost packet costs nothing while a later one arrives.
  Reliable messages cover ready-up, the class, streaks, and cosmetics,
  chat, skipping a killcam, steering a Guided Missile, and switching teams
  in a private lobby.
- **Server to client.** A welcome (player id, tick, snapshot interval, the
  mode and its rules, the server's build, whether it is ranked, custom game
  settings, and whether the player is a spectator), then snapshots. A client
  of another protocol version gets a `Reject` with the server's version; a
  ranked server that turns a player away sends `Denied` with the reason.
- **Reliable game events.** Hits, damage, kills, score, the roster, the
  match clock, the scoreboard, and each mode's objectives, streaks, and
  Zombies events: things that happen once and must not be lost.

`PROTOCOL_VERSION` goes up with every change to the wire format or to rules
the client predicts, and the comment above it lists what each version added.
A client and a server of different versions never connect; the server
browser marks such a server **Newer version** or **Older version**.

## Snapshots and delta compression

A snapshot holds the receiver's own state, every other player's view
(`PlayerView`: quantized, and only what drawing and hit zones need), live
grenades and smoke clouds, the minimap's red dots, and in Zombies the horde.
Each part is sent as changes from the snapshot the client last acknowledged.
Both ends keep about a second of snapshots (`SnapshotHistory`, 64 entries) to
encode and decode against; an acknowledgement older than that gets a full
snapshot.

`shooter_sim::net::delta` does the work for any encoder. The encoder runs
once over the baseline to list its fields, then again over the new value: an
unchanged field costs one bit, a changed float or counter goes as its XOR
with the old value with leading zeros dropped, a number (a position, an
angle, a counter) goes as its difference, and anything else goes raw. The
coder knows nothing about the fields, so a field added to an encoder is
delta-compressed with no further work, and the result is bit-exact.

- **Own state.** The full simulation state, because prediction restarts
  from it (`write_predicted`): about 30 bytes a snapshot.
- **Other players.** Positions in millimetres, view angles in 16-bit steps,
  the shot counter, each small field on its own, and the flags as one field
  (`write_view`): about 8 bytes for a moving player.
- **The Zombies horde.** Sent at 30 Hz (`ZOMBIE_SEND_TICKS`, every 4 ticks),
  positions in centimetres, as changes from the newest horde the client
  acknowledged: about 4 bytes a walking zombie.
- **Inputs.** The first input in a packet is sent whole and each later one as
  changes from the one before. View angles are snapped to whole steps of
  2^-14 radians (about 0.0035 degrees) before the client predicts with them
  (`PlayerInput::quantized`) and travel as step counts, so a turn is a small
  change. Every button and which optional parts follow are one field. The
  camera keeps the unsnapped angle; shots leave from the snapped one.

This is Quake III Arena's model, which Call of Duty inherited:
`MSG_WriteDeltaPlayerstate` in `qcommon/msg.c` sends a "changed" bit per
field against the state the client acknowledged (ideas only; Quake III is GPL
and no code was copied). Quake lists its fields in a table; here the
encoder's call order is the table. Quake III and Call of Duty send command
angles as 16-bit shorts (`ANGLE2SHORT`, 0.0055 degrees); the input grid here
is finer, and yaw is not wrapped. The float XOR is the float compression of
Facebook's Gorilla (Pelkonen et al., VLDB 2015).

## Prediction and reconciliation

`shooter_sim::net::prediction`. The client applies each input at once
through the same `body::tick` the server runs, and remembers each input and
the state it produced for about two seconds. When a snapshot says "after
your input N the server had state S", the client compares S with its own
record for N. If they agree, nothing happens. If not, it restarts from S and
replays every later input, so the correction lands in the present.
Differences under 1 cm in position are float noise, not mispredictions.

A correction moves the camera smoothly rather than at once
(`ErrorSmoothing`): the offset decays at 15 a second, about 90% gone in
0.15 s, and anything over 2 m (a respawn) snaps.

The server never invents input (`shooter_sim::net::input_queue`, the Source
engine "usercmd" model). Each tick it applies the next queued input for each
player, or none, so it runs exactly the inputs the client predicted with.
When packets bunch up after jitter it applies two or three a tick, paid for
with credit that builds one input a tick up to a quarter second, so no
client gets more than one input a tick over time.

## Interpolation

`shooter_sim::net::interpolation`. Other players are drawn between two
snapshots the client already has. `SnapshotClock` estimates the server's tick
from snapshot arrivals and renders at that tick minus a delay sized by the
connection:

- room for one lost snapshot (two snapshot intervals), or the longest recent
  gap between snapshots if longer; the remembered gap shrinks back by 0.6
  ticks a second once packets stop going missing;
- plus 2.5 times the measured arrival jitter.

On a clean link that is about 35 to 50 ms. Every tick of delay is time an
opponent has already moved on the server, so a shorter delay means less
peeker's advantage. The render clock runs up to 10% fast or slow to close on
its target, jumps when it is a quarter second off, and starts over after a
half-second stall. `NetClient.interpolation_ms` sets a floor on the delay,
for trying a longer one.

## Lag compensation

`shooter_sim::net::lag`. Each firing input carries the server tick the
client was drawing other players at (`PlayerInput::view_tick`, in 1/256 of a
tick). The server keeps a second of the snapshots it sent (`LagHistory`) and
rebuilds every target's hit zones at that tick, interpolated exactly as the
client interpolated them (`RemoteSample`). Walls are tested on the server's
map in the present. What a player hit on their screen is a hit: favour the
shooter, as Call of Duty and Source do (Valve, *Latency Compensating
Methods*; Overwatch GDC 2017).

The rewind is clamped to what the shooter's round trip explains plus 0.25 s
(`VIEW_SLACK`), and never more than 300 ms (`MAX_REWIND`). A shooter with a
worse connection leads their targets by the excess, so a high ping cannot
shoot players long gone behind cover on their own screen.

Hit markers show at once from the client's own hitscan; kills, score,
medals, and damage taken come from the server. In a debug build, F4 draws
every hitbox, and the hitboxes the server rewound for each of your shots in
white beside them.

## Sub-tick fire

`shooter_sim::net::subtick`. The client renders faster than the tick and
reads the mouse every frame, so a click between two ticks would otherwise
fire from wherever the next tick's view has turned to. Instead the client
captures the view angles at the press and sends them with the tick's input
(`FireAim`, relative to the tick's angles), along with the remote-player
tick it was drawing at that frame. The first shot on that input leaves from
the press angles, on the predicting client and on the server alike
(`PlayerInput::shot_view`); the view still ends the tick at the tick's
angles. Held automatic fire uses the tick's own angles, since its cadence
comes from the tick. This is Counter-Strike 2's "sub-tick" input.

The server cannot see the mouse, so it accepts press angles only on the
path between the previous input's view and this one's (`subtick::check`):
anything else could be an aimbot snapping the shot while the view stays put.
The client runs the same check before it predicts.

## Hit zones follow the drawn body

Hit zones (`rust/sim/src/hitbox.rs`) are a head sphere and capsules for the
neck, chest, stomach, hips, arms, and legs. They are a pure function of what
a snapshot carries: feet position, view angles, stance, activity (sprinting,
airborne, mantling, vaulting, reloading, aiming, throwing, kneeling at the
charge, prone along a slope, weapon), velocity, and the body's animation
clocks (`hitbox::clock`: stride phase, and reload and mantle progress).
The server never runs animation, and the rewind can rebuild a past pose from
history, the same on every machine.

They still match the animated soldier. Each state starts from a base pose
measured from the soldier's animations (`hitbox/poses.rs`), picked at the
moment of the stride or climb the snapshot names, with the hips turned into
a strafe, the spine and head bent by aim pitch, the weapon where the
character carries it (`hitbox/carry.rs`), and two-bone IK putting the arms
on its grips.

`godot/tests/hitbox_fit_test.gd` poses a real remote player through every
state another player can be seen in and fails if a zone strays from the
drawn body. When an animation changes, regenerate the base poses and run
`cargo fmt` in `rust/`:

```sh
HITBOX_FIT_DUMP=$PWD/rust/sim/src/hitbox/poses.rs \
  godot --headless --path godot --script res://tests/hitbox_fit_test.gd
```

## Zombies

The server runs the horde. Zombies are drawn one zombie send interval
(4 ticks, about 33 ms) behind the other players, so two sent hordes always
bracket the drawn moment, and the server rewinds them by the same amount for
lag compensation (`shooter_sim::net::horde`, `protocol::zombie_lag`).
Player bodies do not collide with zombies, so prediction never runs into a
zombie drawn where the server no longer has it.

## Bandwidth

`godot/tests/bandwidth_test.gd` runs a dedicated server in its own process
and measures what each bot would cost as a client 80 ms of round trip away
(`NetServer.bandwidth_probe_rtt_ms`, `shooter_sim::net::bandwidth::Probe`):
each bot's snapshots are built by the real code and delta-encoded against
the snapshot it would have acknowledged by then, and each tick's input packet
carries every input it would not yet have seen acknowledged. Both are
encoded in the real wire format and counted, payload alone and with IPv4,
UDP, and ENet headers (40 bytes a packet). Reliable game events are not
counted: they are rare next to 60 snapshots and 120 input packets a second.

Two runs: a 6v6 of Normal bots playing Team Deathmatch on Slamcity for 60 s,
and four players against a round-10 Zombies horde (24 zombies out at once)
for 20 s, with zombie strikes made harmless so the scripted players stay up.
It fails past its budgets, so CI catches a regression:

| Run | Down budget | Up budget |
| --- | --- | --- |
| Team Deathmatch, 6v6 | 110 kbps | 70 kbps |
| Zombies, 4 players | 80 kbps | 70 kbps |

```sh
godot --headless --fixed-fps 120 --path godot --script res://tests/bandwidth_test.gd
RTT_MS=30 BOTS=12 SECONDS=60 godot --headless --fixed-fps 120 --path godot --script res://tests/bandwidth_test.gd
```

`-- --tdm` or `-- --zombies` runs one of the two. Typical results per client,
payload kbps with headers in brackets (runs vary by up to 10%, and busier
bots cost more):

| Run | RTT | Down | Own state | Up |
| --- | --- | --- | --- | --- |
| Team Deathmatch, 6v6 | 80 ms | 81 to 85 (100 to 104) | 15 to 16 | 39 to 41 (77 to 79) |
| Zombies, 4 players, 24 zombies | 80 ms | 52 (71) | 19 | 22 (60) |

Up traffic grows with ping, since each packet resends every unacknowledged
input. At 120 packets a second a packet's headers cost more than its
contents.

## Server checks

What the server does with what clients send (`shooter_sim::net::integrity`,
checked in `godot/tests/integrity_test.gd`).

Most cheats are impossible by construction, because the server never takes a
client's word for an outcome:

| A client could try to send | What the server does |
| --- | --- |
| A position, a speed, or a teleport | Nothing to send: the server moves every player itself from their inputs (`body::tick`), with its own collision. |
| Inputs faster than the tick (speed hack) | Inputs cost credit that builds one a tick, a quarter second banked at most (`InputQueue`). |
| A faster fire rate, more ammunition, a shorter reload | The server runs the weapon simulation itself. |
| A lunge to dash, a weapon from afar, a press aim off its path | Lunges must point at a visible enemy, pickups are raycast, press aims must lie on the view's path since the last input (`subtick::check`). |
| An older view tick, to shoot where a target was | Rewinds are clamped to what the round trip explains plus 0.25 s, and to 300 ms. |
| A pitch past straight up | Inputs with a pitch past the limit do not decode. |
| A class or streak the player has not unlocked | A ranked server refuses it ([online.md](online.md)). |
| A chat flood | Rate-limited (`chat::ChatLimiter`). |

What is left is watched two ways.

**Disconnects**, for abusing the protocol, which no honest client does:

- **Packet floods.** A token bucket per client allows 300 packets a second
  (an honest client sends 120 inputs a second and a few events) with a burst
  of 300, enough for a second-long stall delivered at once. Packets over it
  are ignored; 600 ignored within 10 s disconnects the client.
- **Garbage.** A client that sends 10 packets that do not decode within 10 s
  is disconnected. An honest client of the same version never sends one.

**Flags**, logged for an operator (`server: integrity: player N "name"
flagged for ...`), at most once a minute each, and never acted on. A
heuristic can be wrong, so a person decides; Valve's VAC and Call of Duty's
Ricochet likewise gather evidence on the server before anyone is banned.

- **Aim snaps.** A hit whose shot came after the view turned 25 degrees or
  more in the 3 ticks before it (1,000 degrees a second). A controller turns
  at most about 400 degrees a second, and a hard 90-degree mouse flick in a
  tenth of a second is 900. Flagged with at least 5 snap hits among the last
  30 hits, making up at least 30% of them.
- **View ticks.** Firing inputs whose view tick the connection does not
  explain: further back than the round trip plus 0.25 s, or newer than the
  newest snapshot. The 300 ms rewind cap is not counted, so a very slow
  connection is never flagged. Flagged with at least 10 among the last 40
  shots, making up half of them.
- **Lag switching.** Inputs that stop for a fifth of a second or more and
  then arrive in a burst, 10 times within a minute.

The thresholds hold against honest play: twelve Hard bots (whose 420 degrees
a second is a fast player's turn) play 45 s of Team Deathmatch with no flag;
the netcode test plays through 40 ms of latency with 15 ms of jitter each way
and 5% loss without a disconnect; and unit tests run a controller at full
turn boost, repeated 900-degree-a-second flicks, jitter bursts, a
quarter-second stall every 20 s, and a 440 ms round trip without a flag,
against an aimbot snapping 60 degrees onto its hits, a client claiming
half-second-old view ticks, and a switch flipped every two seconds, which
are flagged.

Not covered: wallhacks (snapshots carry every player, which killcams,
spectators, and the minimap rely on) and aim assistance subtle enough to
look human. Both need data the server does not have, or review.

## Trying it

- **F3** in a match shows the netcode overlay (`godot/scripts/net/net_overlay.gd`):
  ping, loss, pending inputs, prediction corrections and the last one's size,
  interpolation delay, jitter, the snapshot gap, and kbps in and out.
- **`--lag MS --jitter MS --loss PERCENT`** after `--connect` simulates a
  link in each direction (`shooter_sim::net::conditioner`; losses and jitter
  come from hashed counters, so a run is reproducible). See
  [multiplayer.md](multiplayer.md).
- **F4** in a debug build draws hitboxes, with the server's rewound ones in
  white.

## Tests

| Test | Checks |
| --- | --- |
| `cargo test` in `rust/` | The protocol (round trips, and garbage that must never panic), delta coding, prediction, interpolation, lag compensation, sub-tick checks, the input queue, and the integrity heuristics |
| `godot/tests/net_test.gd` | A server process with a bot, joined through 40 ms of latency, 15 ms of jitter, and 5% loss: the client joins, plays Team Deathmatch, interpolates the bot, kills it through lag compensation, and predicts its own movement without corrections |
| `godot/tests/subtick_test.gd` | A click between ticks fires from the view at the click; held fire and flicks that the tick never passed through do not |
| `godot/tests/hitbox_fit_test.gd` | Hit zones stay on the animated body in every state |
| `godot/tests/integrity_test.gd` | Hard bots raise no flag; a flooding client and a garbage client are disconnected |
| `godot/tests/bandwidth_test.gd` | Per-client kbps stays under budget |
| `godot/tests/zombies_net_test.gd` | Zombies online: the client draws the server's horde, and a shot at a drawn zombie hits it on the server |

## References

- Gabriel Gambetta, *Fast-Paced Multiplayer* (prediction, reconciliation,
  interpolation, lag compensation).
- Valve, *Latency Compensating Methods in Client/Server In-game Protocol
  Design and Optimization*, and *Source Multiplayer Networking* (usercmds).
- Timothy Ford, *Overwatch Gameplay Architecture and Netcode*, GDC 2017.
- Quake III Arena, `qcommon/msg.c` (delta-compressed snapshots and player
  state).
- Glenn Fiedler, *Gaffer On Games* (snapshot compression).
