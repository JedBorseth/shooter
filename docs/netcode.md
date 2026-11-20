# Netcode: bandwidth and server checks

How much each client costs on the wire, how it is measured, and what the
server checks in what clients send. The design of the netcode itself (tick,
prediction, interpolation, lag compensation) is in [roadmap.md](roadmap.md)
under Phase 2 and Phase 3.

## Measuring bandwidth

`godot/tests/bandwidth_test.gd` runs a dedicated server in-process on Slamcity
with twelve bots playing Team Deathmatch, and measures what each bot would cost
if it were a client behind an 80 ms round trip (`NetServer.bandwidth_probe_rtt_ms`,
`shooter_sim::net::bandwidth::Probe`). Each bot's snapshots are built by the
same code as a real client's and delta-encoded against the snapshot that client
would have acknowledged by then (one a round trip old); each tick's input
packet carries every input it would not have seen acknowledged yet, as
`Prediction::unacked` does. Both are encoded with the real wire format and
counted. It prints kilobits per second per client, payload alone and with
IPv4, UDP, and ENet headers (40 bytes a packet), and fails if a direction
grows past its budget, so CI catches a regression.

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
| Own state as changes (protocol 62) | 80 ms | 92 to 97 (111 to 117) | 14 to 15 | 101 to 103 (139 to 142) |

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
prediction restarts from it. Since protocol 62 it is sent as changes from the
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
