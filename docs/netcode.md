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

Runs vary by a few kbps, since the bots' match differs a little each time.

## Results

Per client, 6v6 Team Deathmatch on Slamcity, 30 s of play, payload kbps
(with headers in brackets). Snapshots go out at 60 Hz and inputs at 120 Hz.

| Change | RTT | Down | Own state | Up |
| --- | --- | --- | --- | --- |
| Before (protocol 57) | 80 ms | 168 (188) | 94 | 106 (144) |
| Before (protocol 57) | 30 ms | 170 (189) | 94 | 53 (91) |

Where it goes, before any change:

- The receiver's own state is the full simulation state, bit-exact, in every
  snapshot: about 196 bytes, more than half of everything sent down.
- The other eleven players cost about 15 bytes each a snapshot: position
  deltas, both view angles in full whenever either changes, and the five
  animation clocks whenever one does.
- Inputs: every packet resends every unacknowledged input, about ten at an
  80 ms round trip, each with both view angles as 32-bit floats. Up traffic
  grows with ping.
