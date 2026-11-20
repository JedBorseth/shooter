# Netcode: bandwidth and server checks

How much each client costs on the wire, how it is measured, and what the
server checks in what clients send. The design of the netcode itself (tick,
prediction, interpolation, lag compensation) is in [roadmap.md](roadmap.md)
under Phase 2 and Phase 3.

## Measuring bandwidth

`godot/tests/bandwidth_test.gd` runs a dedicated server in-process, first on
Slamcity with twelve bots playing Team Deathmatch, then on the Zombies arena
with four players against a round-10 horde (24 zombies out at once; the bench
makes zombie strikes harmless so the scripted players stay up), and measures
what each bot would cost
if it were a client behind an 80 ms round trip (`NetServer.bandwidth_probe_rtt_ms`,
`shooter_sim::net::bandwidth::Probe`). Each bot's snapshots are built by the
same code as a real client's and delta-encoded against the snapshot that client
would have acknowledged by then (one a round trip old); each tick's input
packet carries every input it would not have seen acknowledged yet, as
`Prediction::unacked` does. Both are encoded with the real wire format and
counted. It prints kilobits per second per client, payload alone and with
IPv4, UDP, and ENet headers (40 bytes a packet), and fails if a direction
grows past its budget, so CI catches a regression. Reliable game events
(kills, score, the scoreboard every few seconds) are not counted: they are
rare next to the 60 snapshots and 120 input packets a second.

```sh
godot --headless --fixed-fps 120 --path godot --script res://tests/bandwidth_test.gd
RTT_MS=30 BOTS=12 SECONDS=60 godot --headless --fixed-fps 120 --path godot --script res://tests/bandwidth_test.gd
```

Runs vary by up to 10%, since the bots' match differs a little each time, and
move with the bots' behaviour: busier bots cost more. The budgets leave room
for that (110 kbps down and 70 up in the 6v6, 80 down in Zombies).

## Results

Per client, 6v6 Team Deathmatch on Slamcity, 60 s of play (30 s for the
first rows), payload kbps
(with headers in brackets). Snapshots go out at 60 Hz and inputs at 120 Hz.

| Change | RTT | Down | Own state | Up |
| --- | --- | --- | --- | --- |
| Before (protocol 57) | 80 ms | 168 (188) | 94 | 106 (144) |
| Before (protocol 57) | 30 ms | 170 (189) | 94 | 53 (91) |
| Own state as changes | 80 ms | 92 to 97 (111 to 117) | 14 to 15 | 101 to 103 (139 to 142) |
| Inputs as changes on a view-angle grid | 80 ms | 87 to 88 (107) | 14 | 37 (75 to 76) |
| Other players field by field | 80 ms | 65 to 70 (84 to 89) | 14 | 36 to 37 (74 to 76) |
| The same, after bots stopped getting stuck in rooms (#312) and fight more | 80 ms | 81 to 85 (100 to 104) | 15 to 16 | 39 to 41 (77 to 79) |

Zombies, four players and 24 zombies, 20 s of play:

| Change | RTT | Down | Own state | Up |
| --- | --- | --- | --- | --- |
| Before the horde went as changes | 80 ms | 83 (102) | 19 | 22 (60) |
| The horde as changes | 80 ms | 52 (71) | 19 | 22 (60) |

Where it goes, before any change:

- The receiver's own state is the full simulation state, bit-exact, in every
  snapshot: about 196 bytes, more than half of everything sent down.
- The other eleven players cost about 15 bytes each a snapshot: position
  deltas, both view angles in full whenever either changes, and the five
  animation clocks whenever one does.
- Inputs: every packet resends every unacknowledged input, about ten at an
  80 ms round trip, each with both view angles as 32-bit floats. Up traffic
  grows with ping.

## Own state as changes

The receiver's own state is the whole simulation state, bit-exact, because
prediction restarts from it. Now it is sent as changes from the
own state in the snapshot the client acknowledged, like the other players
(`shooter_sim::net::delta`). The encoder (`write_predicted`) runs once to list
the baseline's fields and again to write the new state against them: an
unchanged field costs a bit, a changed 32-bit field (a float or a counter) is
sent as its XOR with the old value with the leading zeros dropped, and other
fields go raw. The coder knows nothing about the fields, so a field added to
the state is delta-compressed with no further work. Both ends keep each
snapshot's own state in `SnapshotHistory` to decode against. About 196 bytes
a snapshot became about 30.

This is Quake III Arena's model: `MSG_WriteDeltaPlayerstate` in
`qcommon/msg.c` sends a "changed" bit per player-state field against the last
state the client acknowledged, and the field only when it changed (ideas only;
Quake III is GPL and no code was copied). Quake lists its fields in a table;
here the encoder's call order is the table. The float XOR is the float
compression of Facebook's Gorilla (Pelkonen et al., VLDB 2015).

## Other players field by field

Other players were already sent as changes from the acknowledged snapshot,
but in groups: both view angles in full whenever either moved, every flag
whenever one changed, and all five animation clocks whenever the stride
moved, which it does every snapshot a body walks. Now a
player view goes through the same field coder (`write_view`,
`read_view`): positions, angles, and the shot counter as differences, each
small field on its own, and the twelve flags as one field. About 12 bytes a
moving player became about 8.

The Zombies horde went out whole every other snapshot (30 Hz): about 8 bytes
a zombie in centimetres. It now goes as changes from the newest horde the
client acknowledged (the server remembers, per client, the newest
acknowledged snapshot that carried one): a zombie's position as
differences, the rest as one field of flags and counters, and ids that run
in order as a bit each. About 4 bytes a walking zombie.

Every input packet resends every input the server has not acknowledged, so a
lost packet costs nothing; at an 80 ms round trip that is about ten inputs a
packet. Now the first input in a packet is sent whole and each
later one as changes from the one before it, with the same field coder as
the own state. Three things make consecutive inputs differ in few bits:

- View angles are snapped to whole steps of 2^-14 radians (about 0.0035
  degrees) before the client predicts with them (`PlayerInput::quantized`),
  and travel as step counts, so a turn is a small change instead of a new
  32-bit float. Quake III and Call of Duty send user command angles as 16-bit
  shorts (`ANGLE2SHORT`, 0.0055 degrees); this grid is finer, and yaw is not
  wrapped, because the client lets it run past a full turn. The camera keeps
  the unsnapped angle; shots leave from the snapped one, at most 0.0017
  degrees away (1.5 mm at 50 m).
- Every button and which optional parts follow are one 14-bit field, so an
  input whose buttons did not change spends one bit on them.
- The view tick an attacking input carries goes as its whole tick and its
  fraction, so from one input to the next only the whole tick moves, by one.

A packet's headers (IPv4, UDP, ENet: 40 bytes) now cost more than its
contents at 120 packets a second. Sending inputs at 60 Hz, two ticks a
packet, would halve that (about 19 kbps) at the price of up to 8 ms more
input delay on every other tick; that is a design choice left open.

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
| A chat flood | Rate-limited (`chat::ChatLimiter`). |

What is left is watched, two ways:

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
- **Lag switching.** Inputs that stop for a fifth of a second or more and then
  arrive in a burst, 10 times within a minute.

The thresholds were checked against honest play: twelve hard bots (whose
turn rate, 420 degrees a second, is a fast player's) play 45 s of Team
Deathmatch with no flag and no snap hit; the netcode tests play through
110 ms of round trip with 15 ms of jitter and 5% loss without a disconnect;
and unit tests run a controller at full turn boost, repeated 900-degree-a-
second flicks, jitter bursts every half second, a quarter-second stall every
20 s, and a 440 ms round trip without a flag, against an aimbot snapping
60 degrees onto its hits, a client claiming half-second-old view ticks, and
a switch flipped every two seconds, which are flagged.

Not covered: wallhacks (snapshots carry every player, which killcams and the
minimap rely on; sending only what each player can see is a large change),
and aim assistance subtle enough to look human. Both need data the server
does not have, or review.
