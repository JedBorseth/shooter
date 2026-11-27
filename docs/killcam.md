# Killcam

After a death, a replay of the last few seconds through the killer's eyes;
at the end of a match or a Search and Destroy round, the deciding kill
replayed for everyone. Timings follow Call of Duty's killcam script
(`_killcam.gsc`, the same in Call of Duty 4 and Black Ops 3). The rules are
in `rust/sim/src/killcam.rs`, playback in `rust/game/src/net/killcam.rs`, and
the overlay in `godot/scripts/net/killcam_hud.gd`.

## Kinds

| Kind | Who sees it | When |
| --- | --- | --- |
| KILLCAM | The victim | After a death by another player, in modes with respawns, while the match is being played |
| ROUND WINNING KILL | Everyone | After a Search and Destroy round won by wiping out the other team |
| FINAL KILLCAM | Everyone | After the kill that ends a match, or the round that wins a Search and Destroy match |

No killcam plays for a suicide or a fall, in the lobby's warm-up, for bots,
or on a death in Search and Destroy, where the dead watch a teammate
instead. An ending killcam needs a kill at most 8 s
before the end; otherwise none plays. Custom games can turn every killcam
off (`killcam=off`, [custom-games.md](custom-games.md)).

## Timings

| | Seconds | `_killcam.gsc` |
| --- | --- | --- |
| Watching your own death first | 2 | 0.25, then `waitForTimeOrNotifies(1.75)` |
| Shown before a gun or knife kill | 2.5 | `camtime = 2.5` |
| Shown before a grenade kill, for its flight | 4.25 | 4.25 in Black Ops 3, 4.5 in Call of Duty 4 |
| Shown before the kill in an ending killcam | 5 | `camtime = 5.0` |
| Shown after the kill | 2 | `postdelay = 2` |
| Pause before an ending killcam | 1 | `wait 0.35`, rounded up so the result reads first |
| Hold on an ending killcam's last frame | 1.5 | `wait 1.5` in Black Ops 3 |

An ending killcam slows to a quarter speed around the kill
(`setSlowMotion(1.0, 0.25, ...)`): it eases in over the half second before,
holds a quarter second after, and eases back over 0.75 s. The server keeps
the round's result or the match's results up until it has played.

## Skipping

A respawn killcam can be skipped with interact (F) or the reload button
(X / Square), as Call of Duty's "Press [Use] to skip". The **Skip killcams**
setting (Options, General) skips every one after the death camera. Ending
killcams cannot be skipped.

The server holds the victim's respawn until the killcam ends or is skipped
(the client says so with `ClientMessage::SkipKillcam`), then respawns them
as soon as the respawn delay allows. A client that never answers is
respawned after the death delay, the replay, and a second's margin
(`killcam::respawn_wait`).

## How it plays

The replay costs no bandwidth. Snapshots already carry every player to every
client, so each client keeps the last 16 s of them (`killcam::Tape`), its own
state and the grenades and smokes included. The server only says which kill
to show (`GameEvent::Killcam`): killer, victim, tick, weapon and attachment,
cause, headshot, and the lag-compensation rewind the killer fired with, so
the replay draws everyone else as the killer saw them and the shot lands
where it did.

The client draws the replay with its own copies of the players, so the live
ones keep following the match underneath, hidden and silent. The killer is
drawn from their eyes: their body hides, and the local first-person rig
plays their weapon, aim, reloads, switches, shots, and knife
(`ViewDriver`). Grenades fly and burst in the replay. Bodies that fall in it
go when it ends, and bodies already lying in the world hide while it plays.

## On screen

While a killcam plays, the rest of the HUD steps aside and
`killcam_hud.gd` draws:

- the title at the top centre, with a thin bar filling as the replay plays;
- the killer's card at the bottom centre: their name in the colour we see
  them in, the weapon and attachment, and a headshot mark; an ending
  killcam also names the victim;
- "[F] Skip" under the card, on a respawn killcam only.

Call of Duty shows the same parts: a KILLCAM title, the killer's name card
with the weapon and attachments, and the skip prompt.

## Differences

Call of Duty plays a killcam on death in Search and Destroy and then lets
the dead spectate; here the dead go straight to watching a teammate, by the
project's choice, and the round's last kill is replayed for everyone.

## Tests

- `rust/sim/src/killcam.rs`: timings, slow motion, the tape, and which
  deaths earn a killcam.
- `godot/tests/killcam_test.gd`: a bot's kill replayed and skipped on a
  server.
