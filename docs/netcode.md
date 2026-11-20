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

Runs vary by up to 10%, since the bots' match differs a little each time.

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
