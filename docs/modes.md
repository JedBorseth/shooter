# Multiplayer modes

Every multiplayer mode, the match flow they share, and how players score.
Domination ([domination.md](domination.md)) and Search and Destroy
([search-and-destroy.md](search-and-destroy.md)) have their own pages, as do
spawns ([spawns.md](spawns.md)), scorestreaks
([scorestreaks.md](scorestreaks.md)), killcams ([killcam.md](killcam.md)),
and custom games ([custom-games.md](custom-games.md)).

The rules live in `rust/sim` (`tdm.rs` for the modes, match clock, periods,
and spawns; `score.rs`, `hardpoint.rs`, `ctf.rs`, `gun_game.rs`,
`domination.rs`, `snd.rs`). The dedicated server runs them
(`rust/game/src/net/server.rs`) and clients draw what it sends.

## The modes

| Mode | `--mode` | Win | Default limit | Clock | Respawn |
| --- | --- | --- | --- | --- | --- |
| Team Deathmatch | `tdm` | Team kills | 75 | 10 min | 3 s |
| Free-for-all | `ffa` | Own kills | 30 | 10 min | 3 s |
| Capture the Flag | `ctf` | Captures | 3 | 10 min, two halves | 5 s |
| Hardpoint | `hardpoint` | Points | 250 | 10 min | 3 s |
| Domination | `dom` | Points | 200 | 10 min, two halves | 3 s |
| Gun Game | `gungame` | Last weapon on the ladder | 8 weapons, 2 kills each | 10 min | 3 s |
| Search and Destroy | `snd` | Rounds | 6 | none (1:45 rounds) | none in a round |

Servers take `--score-limit N` (0 keeps the mode's), `--time-limit MINUTES`,
`--rounds N` (Search and Destroy), and `--kills-per-weapon N` (Gun Game, 1
to 10), all parsed in `godot/scripts/net/bootstrap.gd`. The title screen's
match setup (`godot/scripts/tdm_setup.gd`) offers each mode with its own
limits, a map, team sizes, bot difficulty, and the custom game settings.
Search and Destroy's setup lists only Breakwater, the map with charge
sites. An unknown `--mode` runs Team Deathmatch.

A server accepts up to 12 players (bots are not connections). In team modes a new player joins the
smaller team (ties go to team 0 for humans and team 1 for bots,
`tdm::assign_team`). With `--bots N` and a bot skill, bots fill the server
to N + 1 players and each human who joins takes a bot's place; `--team-bots`
sets each team's bots exactly ([custom-games.md](custom-games.md)).

## Match flow

A match goes through four phases (`tdm::Phase`), which the server sends in
`GameEvent::Match`:

1. **Lobby.** Players join, pick a class, and can run and shoot, but nothing
   scores. The clock reads WARM-UP. The class menu (L, or D-pad up) opens on
   joining and holds the Ready button; the match counts down once every
   human is ready. `--auto-start` skips the ready-up.
2. **Countdown.** 5 s. The banner reads MATCH STARTS IN N with the mode and
   its limit ("Team Deathmatch · first to 75 kills").
3. **Playing.** Scores, clocks, and streaks count. The first to the limit
   wins; when the clock runs out the leader wins, and level is a draw.
4. **Ended.** 15 s of results: VICTORY, DEFEAT, or DRAW (the winner's name
   in a free-for-all), the match's XP beside it, and the final killcam
   first. Then the next match counts down by itself; only a server's first
   match waits for ready-ups.

When every human leaves, the server goes back to the lobby and resets the
map's props.

### Periods

Every match has a period (`tdm::Period`), sent with the match state:
regulation, the second half (Capture the Flag and Domination), or overtime
(Capture the Flag and Hardpoint). Overtime is timed (its own clock, shown
as OT and the time) or untimed, when the server ends it once the mode's
condition lapses (the clock reads OVERTIME). The match HUD calls each new period in the middle of
the screen: HALFTIME ("Switching sides") or OVERTIME with what it means for
us ("Next capture wins", "Stay in the zone", "Clear the zone to win").

### Stings

`godot/scripts/net/match_stings.gd` plays a sting when a match goes live,
its result (win, loss, or draw), a warning at 30 s left (not in Search and
Destroy), and in Search and Destroy each round's start and result.
Halftime reuses the round-start sting and overtime the time warning. There
is no announcer.

## Score and medals

The server keeps every player's score (`score::ScoreState`) and sends each
award and the new totals (`GameEvent::Score`). Kills pay a flat 100, as in
Black Ops 2, with bonuses on top:

| Award | Points | When |
| --- | --- | --- |
| Kill | 100 | any kill of an enemy |
| Headshot | +50 | the killing hit was a headshot |
| Longshot | +50 | 40 m or more |
| Point Blank | +25 | 2.5 m or less |
| Collateral | +100 per extra kill | one shot killed more than one |
| Double Kill, Triple Kill, Quad Kill, Nx Multi Kill | +50 per extra kill | kills within 4 s of each other |
| N Kill Streak | +100 | every 5 kills without dying |

Each mode adds its own awards: Capture the Flag and Hardpoint below,
Domination and Search and Destroy on their pages. Shooting down an enemy
scorestreak pays 100 and a "Destroyed ..." medal
([scorestreaks.md](scorestreaks.md)).

Headshots, distance bonuses, and objective ticks show as "+N" lines under
the crosshair; the rarer awards are medals with their own drawn emblem and
chime (`Award::is_medal`, `godot/scripts/score_feed.gd`). A suicide, a fall,
or a teamkill scores nothing. Score earned counts toward scorestreaks.

## Team Deathmatch

Two teams race to 75 kills in 10 minutes; each kill of an enemy is a team
point (`Mode::kills_score`). Black Ops 2's Team Deathmatch plays to 7,500
score (75 kills at 100 each) in 10 minutes.

## Free-for-all

Every player for themselves, with their own class and scorestreaks. The
first to 30 kills wins; at the buzzer the most kills wins, and a tie for the
lead is a draw (`tdm::kill_leader`). Everyone is on team 0 and every spawn
point is anyone's. Black Ops 2's Free-for-all plays to 30 kills in 10
minutes.

## Hardpoint

| Rule | Here | Call of Duty |
| --- | --- | --- |
| Zone | One live zone; a team alone in it scores a point a second; contested or empty scores nothing | Same in every Hardpoint since Black Ops 2 |
| Rotation | Every 60 s to the next zone in the map's fixed order, wrapping around; the new zone starts neutral | 60 s in Black Ops 2 onward public play and the competitive rules; zones go in a fixed order players learn |
| Limit | 250 points or 10 minutes | 250 points; Black Ops 6 public play |
| Overtime | When the clock runs out while a team that is not ahead has a living player in the live zone (holding or contesting), play goes on. The zone keeps scoring and rotating. Overtime ends the moment no team that is not ahead stands in the zone; the team ahead then wins (a draw if level). A trailing team holding the zone alone can score past the leader and win | See below |

**Player score.**

| Award | Points | When |
| --- | --- | --- |
| Hardpoint Captured (medal) | 100 | standing in the zone when the team takes it |
| Securing Hardpoint | 50 | every 5 s in a zone the team holds |
| Hardpoint Defense | +50 on the kill | the killer or the victim on or within 5 m of the zone, which the killer's team held last |
| Hardpoint Assault | +50 on the kill | the same, when it did not |

The scoreboard adds a TIME column: seconds spent in the live zone.

**Zones.** Each map lists its zones in rotation order under a `Hardpoints`
node: a `Marker3D` per zone, its box in `size` metadata (default
10 × 4 × 10 m), its HUD name in `label`. As on Call of Duty's maps, the
first zone is in the middle and the rotation swings from one side of the map
to the other, so neither team's spawn gets two near zones in a row.
`godot/tests/hardpoint_zones_test.gd` checks every map: four or five zones,
every move at least 16 m to a zone whose footprint does not touch the last
one's, and most of each footprint standable.

| Map | Rotation |
| --- | --- |
| Testing Area | Crossroads, The Platform, Tunnel, Target Range |
| Warehouse | Mid Stockpile, Receiving, Pick Mezzanine, Catwalk, Quality Control |
| Breakwater | Loading Rack, Pump House, Rack Deck, Fire Pumps |

A map without `Hardpoints` gets one zone halfway between the teams' spawns.
The live zone counts as two enemies at its centre when choosing spawns, so
nobody spawns on the hill.

**Overtime.** This is the project's rule ("overtime when the trailing team
is in the zone at the buzzer"). Call of Duty's public Hardpoint ends at the
buzzer, and the official mode guides for Modern Warfare III and Black Ops 6
describe no Hardpoint overtime, so the details follow the rule's plain
reading: holding and contesting both keep it alive, and it has no clock of
its own. `hardpoint::overtime_holds` decides it;
`NetServer::hardpoint_overtime` starts and ends it.

**On screen** (`godot/scripts/net/hardpoint_view.gd`): the zone's outline
and a low wall of light in the holder's colour; a marker with its distance
and CAPTURE, DEFEND, ATTACK, or CONTESTED; a strip under the score block
with the zone's name, the time to the move, and who holds it; calls
(HARDPOINT ONLINE, WE HAVE THE HARDPOINT, ENEMY HAS THE HARDPOINT,
HARDPOINT CONTESTED, HARDPOINT LOST, HARDPOINT MOVING IN N); and in the last
10 s a dim NEXT marker on the next zone.

**Bots** rotate early: 12 s before a move, a third of a team's bots head
for the next zone while the team holds the live one, half when it does not
(`hardpoint::bots_rotating`). Black Ops 2's bots wait for the move; players
do not. See [bots.md](bots.md).

## Capture the Flag

Reference game: **Black Ops 2**, as for Domination.

| Rule | Here | Call of Duty |
| --- | --- | --- |
| Flags | Take the enemy's flag from their base or where it lies (within 1.4 m); capture within 2 m of your own base, only while your own flag is home | Same since Call of Duty 4's CTF |
| Returns | Touch your own dropped flag to send it home at once; left alone it goes home after 30 s. A carrier who dies or leaves drops it where they were | Black Ops 2 returns on touch; dropped flags reset on their own after a timeout (about 30 s, not sourced exactly) |
| Limit | 3 captures or 10 minutes | 3 captures; Black Ops 2 plays two 5-minute halves |
| Halftime | At half the clock the teams swap bases, both flags go home, everyone starts again from their new side, and the score carries over (`ctf::halftime_due`, `Flags::set_sides`) | Black Ops 2: two rounds with the sides switched |
| Overtime | Level when the clock runs out: an overtime round as long as a half, from the first half's bases, where the next capture wins; no capture is a draw | Black Ops 2 plays overtime rounds with the sides switched each round (David Vonderhaar, Gameranx, 22 October 2012) |
| Respawn | 5 s, so clearing a base buys time to take its flag | |
| Spawns | Each team spawns only on its own side while any point there exists | |

**Player score.**

| Award | Points |
| --- | --- |
| Flag Taken | 100 |
| Flag Capture (medal) | 500 |
| Flag Return (medal) | 100 |
| Carrier Kill (medal): killing the enemy carrying your flag | 100 |

The scoreboard adds CAPS and RET columns.

**Bases.** A map marks them with `Marker3D` children of a `Flags` node with
`team` metadata 0 or 1 (Breakwater). Other maps put each base
at the middle of that team's spawn points.

**On screen** (`godot/scripts/net/ctf_view.gd`): each flag on a modelled
pole at its base, where it lies, or over its carrier; a ring at each base;
markers (TAKE, DEFEND, RETURN, KILL, ESCORT, CAPTURE) with distances; both
flags' status beside the score block; a banner and chime for every take,
drop, return, and capture, with a line in the kill feed.

**Differences.** Black Ops 2's League Play settles a draw with a two-round
stopwatch. That is a competitive option, not the public rule, so one
sudden-death round is played here instead.

## Gun Game

| Rule | Here | Call of Duty |
| --- | --- | --- |
| Ladder | Eight weapons, two kills with each (`gun_game::LADDER`): LSW-6, AK-47, M4A1, BR-3, PDW-9, M870, M40, M1911 | Black Ops: 20 weapons, one kill each; Modern Warfare (2019) onward: 18 to 20 |
| Promotion | The next weapon comes up at once, mid-life, with a full magazine | Same |
| Knife | A knife kill sets the victim back a weapon and does not move the killer up | Black Ops "Humiliation": a melee or ballistic knife kill sets the victim back |
| Suicide | Dying by your own hand (a fall) sets you back a weapon | Same in Black Ops |
| End | The last weapon's kills win; at the buzzer the player furthest up wins, and of players level, the one who got there first | Same |
| Loadout | The ladder's weapon only: no class, perks, or scorestreaks | Same |

**Calls.** A new weapon shows its name in the middle of the screen with
WEAPON N / 8; a set-back shows SET BACK; anyone reaching the last weapon is
called in the kill feed. The class menu shows the ladder in place of
classes, and the scoreboard is one table ordered by weapon. Call of Duty's
announcer calls each upgrade and each humiliation; there is no announcer
here.

**Differences.** Black Ops ends its ladder on the crossbow and the ballistic
knife, and later games on a melee weapon; here it ends on the M1911.

## Calls across modes

Every mode calls the same moments the same way: the countdown banner names
the mode and its limit, the match HUD calls halftime and overtime from the
period, Search and Destroy's round results use the same banner, and the
result is VICTORY, DEFEAT, or DRAW with its sting.
