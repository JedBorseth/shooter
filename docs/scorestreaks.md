# Scorestreaks

Rewards earned with score in one life, as in Black Ops 2 (kept in Black Ops 3 and Black Ops 4). The rules are in `rust/sim/src/streak.rs`, the minimap's side in `rust/sim/src/radar.rs`, and the server's in `rust/game/src/net/server.rs` (`update_streaks`, `use_streak`).

## Rules

- **Choosing:** a player brings up to three streaks, chosen under the class list in the match menu (L), shared by every class. Black Ops 2's Scorestreaks menu is also separate from the classes. A new choice counts from the next spawn. New players bring the UAV and the Counter-UAV.
- **Earning:** every point scored since the last spawn counts toward them, objective play included: kills and their bonuses, flag captures, Hardpoint time, plants and defuses. A streak is earned when the life's score reaches its cost. After the dearest one is earned, the count starts again from nothing, so a long life earns them all again. Points scored while dead do not count.
- **Keeping:** earned streaks are held until used, through death, until the match ends. Up to 8 are held at once.
- **Death** resets the count.
- **Using:** the use key calls in the selected held streak. The next key selects another. Earning one selects it, as in Black Ops 2.
- **Modes:** every mode but Gun Game and Zombies, as in Black Ops 2's party modes and Zombies. Search and Destroy keeps both the count and held streaks across rounds within a life, as Black Ops 2 does.
- **Bots** bring the UAV, the Care Package, and the Airstrike, and call in what they earn at once.

| Streak | Cost | Black Ops 2 | Effect |
| --- | --- | --- | --- |
| UAV | 425 | UAV, 425 after patch 1.04 (350 at launch) | sweeps every enemy onto the team's minimaps every 3 s for 30 s |
| Counter-UAV | 475 | Counter-UAV; sources give 400 to 600 | jams the enemy's minimaps for 30 s |
| Care Package | 550 | Care Package, 550 | a crate dropped from a flyover, with a random streak inside |
| Airstrike | 750 | Lightning Strike, 750 | three strikes on points marked on the map |

## UAV

While a side's UAV is up, it sweeps every 3 seconds. Each living enemy shows on that side's minimaps as a red dot where they stood at the sweep, fading out before the next sweep. Several UAVs on one side overlap rather than speeding up, and the side shows the effect while any is up.

- **Ghost** hides a player from sweeps while they move faster than 0.5 m/s, plant, or defuse. Standing still shows them, as in Black Ops 2.
- **Unsuppressed fire** still shows a Ghost player.

Black Ops 2's UAV lasts 30 seconds. The 3 s sweep is an estimate of its refresh.

## Counter-UAV

While an enemy Counter-UAV is up, the minimap shows static and no dots at all, not even teammates. That covers fire, sweeps, and our own side's UAV. **Hard Wired** players keep their minimap. The 30 s duration is an estimate in line with Black Ops 2.

## Care Package

Calling one in marks the floor where the caller aims, up to 40 m off, or their feet if that spot has no open sky above it. If neither is open, the streak stays held. Black Ops 2 throws a marker grenade instead; aiming does the same job without a throw animation.

A cargo aircraft (a Blender model, `tools/gear_models.py` `cargo_aircraft`) crosses the map 40 m over the spot along a random heading at 45 m/s. It lets the crate go over the spot 4 s after the call, and the crate falls for 2.5 s. The crate is `care_package` in the same file: a reinforced supply crate on skids with straps and an amber beacon.

Anyone holding use within 1.8 m of a landed crate captures it:

- **Times:** the caller in 0.5 s, anyone else in 3 s. A teammate, or an enemy stealing it, takes the same 3 s. Call of Duty's rule since Modern Warfare 2, kept in Black Ops 2.
- **One at a time:** the first player to start holds the capture until they let go or step away.
- **Running out:** a crate nobody takes is gone after 90 s (an estimate).
- **Limit:** at most 8 crates wait at once; the oldest goes first.

**Contents:** the streak inside is rolled when the crate is called in, from the UAV (106) and the Counter-UAV (94), Black Ops 2's own weights for those two, and the Airstrike (40, an estimate: rare, as the dear streaks are in Black Ops 2). A crate never holds another Care Package. Black Ops 2's booby-trapped crates (Engineer, Black Hat) are left out: there is no Engineer.

**On screen:**

- "[F] HOLD FOR UAV" under the crosshair beside a crate, and a bar while capturing it (`godot/scripts/net/care_package_view.gd`).
- Landed crates show on the minimap: ours blue, anyone else's white.

**Bots** call theirs in where they aim, run to a landed crate (their own within 40 m, anyone else's within 15 m), and hold use on it. `care_package_test` plays it through on a server.

## Airstrike

The lethal streak, after Black Ops 2's Lightning Strike, under a generic name.

![The Airstrike's target picker with two marks and the cursor](media/airstrike-picker.jpg)

**Picking targets:** calling it in opens the whole map's plan full screen (`godot/scripts/net/airstrike_view.gd`). The player stands still meanwhile, as with Black Ops 2's tablet. The plan shows us and our teammates. Each mark picks a point, and the third mark calls the strike in.

| Action | Mouse | Controller |
| --- | --- | --- |
| Mark | left click | A, moving the cursor with the left stick |
| Undo the last mark | right click | X |
| Close (the streak stays held) | Escape | B |

**Strikes:**

- **Timing:** strikes land in turn, the first 3 s after the call and the rest 1 s apart.
- **Where:** each hits the topmost surface under its mark. A roof shields the people under it, because the blast needs a clear line, as grenades' do.
- **Blast:** 250 damage at the centre, falling to 40 at 9 m.
- **Friendly fire:** none, but the caller can hit themselves.
- **Visuals:** a jet (`tools/gear_models.py` `strike_jet`) passes low over each mark just before its blast, three times a frag's size, with its bang.

Its kills score as usual and show as "Airstrike" in the kill feed, but add nothing to the caller's streak count. Black Ops 2's rule is that streak kills do not count toward streaks. The timings and blast size are estimates: Black Ops 2 publishes none.

`airstrike_test` plays it through on a server. Bots mark their nearest enemies.

## On screen

- **Streak column:** the three streaks, bottom right above the weapon cluster, the cheapest lowest, as Black Ops 2 stacks them. Each shows its cost and a thin bar filling with this life's score toward it. An earned streak is lit, with a count when more than one is held. The selected one carries the use key's cap (`godot/scripts/net/streaks_hud.gd`).
- **"UAV READY"** shows in the upper third with the key to call it in.
- **Calls** when a UAV or Counter-UAV goes up: "UAV ONLINE", "ENEMY UAV OVERHEAD", "COUNTER-UAV ONLINE", "ENEMY COUNTER-UAV". These stand in for Black Ops 2's announcer until there are voice lines.
- **Minimap tags:** small tags along the minimap's foot show every UAV and Counter-UAV up, ours blue and the enemy's red.

![The streak column with a held Airstrike and the UAV's bar filling](media/scorestreaks-hud.jpg)

![The minimap jammed by an enemy Counter-UAV](media/counter-uav-jammed.jpg)

## Controls

| Action | Keyboard | Controller |
| --- | --- | --- |
| Use scorestreak | 4 | D-pad right |
| Next scorestreak | 5 | D-pad left |

Black Ops 2 puts scorestreaks on the D-pad, and on 4 and the mouse wheel on PC. The D-pad left and right no longer pick the primary and secondary weapons; Y switches weapons, as in Call of Duty.

## Server authority

The server keeps every count and held streak and decides every use (`ClientMessage::UseStreak` names a held streak; a use of one not held is ignored). Each player hears of their own count and held streaks (`StreakEvent::State`), and everyone of the UAVs and Counter-UAVs up (`StreakEvent::Sky`). Each snapshot carries only the dots that player's radar shows (`RadarView`). `streaks_test` plays it through on a server.

## Still to come

Blind Eye (with an AI streak to hide from), UAV assist points, and announcer lines. They are tracked in [TODO.md](../TODO.md).

## Call of Duty reference

- **Black Ops 2's scorestreak rules:** score earned in one life, objective score included; earned streaks kept after death; streak kills not counting toward streaks; the counter starting again after the last streak.
- **Costs:** UAV 425 (raised from 350 in patch 1.04), Counter-UAV between 400 and 600 depending on the source (475 here), Care Package 550.
- **Care Package:** a flyover drops a crate on the marked spot. The owner captures almost at once, others in about 3 s, and enemies may steal it. Contents are weighted toward cheap streaks (UAV 10.6%, Counter-UAV 9.4%).
- **Durations:** UAV and Counter-UAV 30 s.
- **Hardline:** in Black Ops 2 it adds 20% to scorestreak score, per the Call of Duty wiki, about 16.7% off each cost. Guides that give "-25 points" or "15%" disagree with it.
- **Lightning Strike:** 750, three strikes on points picked on the map, in sequence.
- **Streak kills:** do not count toward streaks.
- **Ghost:** works while moving. **Hard Wired:** immune to Counter-UAV.
