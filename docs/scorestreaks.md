# Scorestreaks

Rewards earned with score in one life, as in Black Ops 2 (kept in Black Ops 3 and Black Ops 4). The rules are in `rust/sim/src/streak.rs`, the minimap's side in `rust/sim/src/radar.rs`, and the server's in `rust/game/src/net/server.rs` (`update_streaks`, `use_streak`).

## Rules

- **Choosing:** a player brings up to three streaks, chosen under the class list in the match menu (L), shared by every class. Black Ops 2's Scorestreaks menu is also separate from the classes. A new choice counts from the next spawn. New players bring the UAV and the Counter-UAV.
- **Earning:** every point scored since the last spawn counts toward them, objective play included: kills and their bonuses, flag captures, Hardpoint time, plants and defuses. A streak is earned when the life's score reaches its cost. After the dearest one is earned, the count starts again from nothing, so a long life earns them all again. Points scored while dead do not count.
- **Keeping:** earned streaks are held until used, through death, until the match ends. Up to 8 are held at once.
- **Death** resets the count.
- **Using:** the use key calls in the selected held streak. The next key selects another. Earning one selects it, as in Black Ops 2.
- **Modes:** every mode but Gun Game and Zombies, as in Black Ops 2's party modes and Zombies. Search and Destroy keeps both the count and held streaks across rounds within a life, as Black Ops 2 does.
- **Bots** bring the UAV and two of the Care Package, the Guided Missile, the Airstrike, the Sentry Gun, and the Attack Chopper, varied between bots, and call in what they earn at once (trying another if one cannot go there).
- **Roofed maps:** a map roofed over from wall to wall (Warehouse) has no open sky for a crate, a strike, a missile, or a helicopter's gun to come down through. There the Care Package, the Guided Missile, the Airstrike, and the Attack Chopper are left out: they are greyed out in the streak column with "NO OPEN SKY" in place of the cost, the count runs on the other chosen streaks (the dearest of those closes the loop), and the server refuses to call one in. The radar streaks still work. Black Ops 2 has no roofed map, so there is no precedent; this stands until the owner decides on a skylight or roof hatch for Warehouse. A map is marked by `roofed = true` metadata on its scene's root (`Streak::needs_open_sky` lists the streaks it leaves out).

| Streak | Cost | Black Ops 2 | Effect |
| --- | --- | --- | --- |
| UAV | 425 | UAV, 425 after patch 1.04 (350 at launch) | sweeps every enemy onto the team's minimaps every 3 s for 30 s |
| Counter-UAV | 475 | Counter-UAV; sources give 400 to 600 | jams the enemy's minimaps for 30 s |
| Care Package | 550 | Care Package, 550 | a crate dropped from a flyover, with a random streak inside |
| Guided Missile | 700 | Hellstorm Missile, 700 | a missile flown down from its nose camera, boosted, and split into bomblets |
| Airstrike | 750 | Lightning Strike, 750 | three strikes on points marked on the map |
| Sentry Gun | 800 | Sentry Gun, 800 | an automated gun set down in front of the caller, firing on enemies in front of it for 4 minutes |
| Attack Chopper | 1100 | Stealth Chopper, 1100 | an AI helicopter guarding a point marked on the map for a minute |

## UAV

While a side's UAV is up, it sweeps every 3 seconds. Each living enemy shows on that side's minimaps as a red dot where they stood at the sweep, fading out before the next sweep. Several UAVs on one side overlap rather than speeding up, and the side shows the effect while any is up.

- **Ghost** hides a player from sweeps while they move faster than 0.5 m/s, plant, or defuse. Standing still shows them, as in Black Ops 2.
- **Unsuppressed fire** still shows a Ghost player.

Black Ops 2's UAV lasts 30 seconds. The 3 s sweep is an estimate of its refresh.

## In the sky

A UAV or Counter-UAV is an aircraft in the world, not only an effect. Each circles the map on a ring both the server and every client compute from the same few numbers (`streak::Orbit`), so the server sends only when it was called and where round the ring it started.

- **The ring:** centred over everywhere players spawn, walk, and fight, 45 m above the highest of those points. The UAV flies four fifths of the way to the far edge (20 to 60 m out), clockwise seen from above, once a minute. The Counter-UAV flies a fifth closer in and 10 m higher, the other way round, every 40 s. Black Ops 2 turns its UAV rig once a minute clockwise and its Counter-UAV rig every 40 s the other way, the planes about 4,000 to 5,000 units out (100 to 127 m) and the Counter-UAV's rig 400 units higher (`_spyplane.gsc`). Our maps are smaller, so the ring is scaled to the map and stays in a launcher's reach.
![The UAV circling overhead](media/uav-overhead.jpg) ![An enemy Counter-UAV overhead](media/counter-uav-overhead.jpg)

- **The models:** both are built in Blender (`tools/gear_models.py`). The UAV (`uav_drone`) is a long-winged surveillance drone with a sensor turret and a pusher propeller. The Counter-UAV (`counter_uav_drone`) is a darker swept flying-wing jammer with emitter pods. Both bank into the turn (`godot/scripts/net/sky_view.gd`).
- **Shooting one down:** enemies can shoot down the other side's (anyone but the caller, without teams). Each has 700 health, Black Ops 2's `u2_maxhealth`. Bullets take off their damage at that range, and the server checks them against a 3.2 m sphere where the shooter saw the aircraft, rewound as players are. A rocket that passes within 6 m downs any of them at once, as in Black Ops 2, where a rocket's damage is the health plus one. Shooting one gives a hit marker.
- **Downed:** the streak ends at once for its side, so a downed UAV stops sweeping and a downed Counter-UAV stops jamming. It blows up where it was hit and its wreck falls away. The shooter scores 100 and the "Destroyed UAV" (or "Destroyed Counter-UAV") medal. Everyone hears "ENEMY UAV DESTROYED" or "UAV SHOT DOWN" in the upper third. Black Ops 2 publishes no point value for this. Earlier games paid 100, and one Black Ops 2 source gives 75.
- **Bots:** one bot in four carries the RL-4 as its sidearm. Black Ops 2's bots pick their secondary at random from every one there is, and three of those are launchers (`_bot_loadout.gsc`). With no enemy player in sight, a bot with a launcher turns it on enemy aircraft it can see that are no more than 70° overhead (Black Ops 2's bots let higher ones go, `_bot_combat.gsc`). It switches to the launcher, aims ahead of the aircraft for the rocket's flight, fires when on target, and switches back when the sky is clear or an enemy shows. `air_streaks_test` plays all of this through on a server.

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

## Guided Missile

The Guided Missile follows Black Ops 2's Hellstorm Missile (`_remotemissile.gsc`), under a generic name. The rules are in `rust/sim/src/missile.rs`, the server's side in `update_missiles`, and the client's in `godot/scripts/net/missile_view.gd`. The missile is a Blender model (`tools/gear_models.py` `guided_missile`) with the RL-4 rocket's flame and smoke trail.

![Flying the Guided Missile](media/guided-missile.jpg)

- **Launch:** it starts high over a point 38 m ahead of its caller, 110 m up and 43 m back, diving at the point. Black Ops 2 aims 1,500 units ahead and starts 18,000 units up and 7,000 back. We keep that 69° dive and scale the height to our maps. The caller's view goes to the missile's nose camera, and their body stands where it was, still open to fire.
- **Flying:**
  - the look controls (mouse or right stick) turn its nose at up to 1.4 rad/s;
  - it only ever dives, between 10° and 89° below level;
  - it flies at 26 m/s, and holding aim boosts it to 65 m/s until it splits;
  - it goes off after 9.95 s in the air if it meets nothing (`missile_timeout_watch`).

  The client sends its steering about thirty times a second (`ClientMessage::SteerMissile`). The server flies the missile and decides every blast.
- **The split:** fire splits it, as in Black Ops 2:
  - one bomblet goes to each enemy within 15 m (600 units) of where the nose points;
  - random ones fill to nine, 5 to 14 m out (200 to 550 units) in sixths of a circle;
  - one more goes straight ahead;
  - each leaves a frame (50 ms) after the one before and flies straight at 70 m/s.

  The missile itself coasts on, unseen, and still goes off where it lands. Enemies near the middle of the view are boxed in red, brighter within the cluster's reach.
- **Blasts:** the missile does 300 damage at its centre, falling to 50 at 9 m. A bomblet does 160, falling to 30 at 4.5 m. Both go off on the map or on an enemy they meet. Their kills show as "Guided Missile" and count nothing toward streaks.
- **The camera** comes back a second after the first bomblet lands (Black Ops 2's `bomblet_camera_waiter`), or half a second after an unsplit missile goes off. It comes back at once if the pilot dies; the missile flies on unsteered.
- **Bots:** a bot aims it at its nearest enemy within 80 m, steers it at them, and splits it a second and a half in, as Black Ops 2's bots press fire 1.5 s into theirs. It stands still meanwhile.
- **Roofed maps** leave it out.
- **Estimates:** the speeds, turn rate, blasts, and bomblet speed are estimates. Black Ops 2 keeps them in weapon files its scripts do not carry.

`guided_missile_test` plays it through on a server.

## Attack Chopper

The Attack Chopper follows Black Ops 2's Stealth Chopper (`_helicopter.gsc`, `helicopter_comlink`), under a generic name. Of the two AI helicopters Black Ops 2 has, the Stealth Chopper guards a point with its gun; the Hunter Killer is a homing drone. The rules are in `rust/sim/src/chopper.rs`, the server's side in `update_choppers`, and the client's in `godot/scripts/net/chopper_view.gd`. Its body and rotor are Blender models (`attack_chopper`, `chopper_rotor`).

![An Attack Chopper guarding its point](media/attack-chopper.jpg)

- **Calling it in:** it opens the Airstrike's map picker for one mark, the point to guard, as Black Ops 2 has the player pick it on the map. Bots mark a random enemy's spot, as Black Ops 2's bots do. Only one a side flies at once (the "chopper" rule); a second stays held.
- **Its flight:**
  - it flies in from 90 m out at 25 m/s, 22 m over the map's highest floor (Black Ops 2's air support height is about 850 units);
  - it hovers 12 s at a spot (`heli_protect_pos_time`), then moves at 10 m/s to another random spot 4 to 20 m from the point (Black Ops 2's 20% to 100% of 2,000 units, scaled to our maps);
  - after 60 s (`heli_protect_time`) it flies back out.

  Both ends compute the whole path from the call's tick and a seed.
- **Its gun:**
  - it picks a target at most every 0.6 s: the nearest enemy it can see within 89 m (3,500 units);
  - it turns onto them, spins up for 0.7 s, then fires bursts of 20 with 1.5 s between;
  - players spawned in the last 5 s are left alone (`heli_target_spawnprotection`).

  The rate (600 rounds a minute) and damage (40 a round, three to kill) are estimates: Black Ops 2 keeps them in the gun's weapon file. Its kills show as "Attack Chopper" and count nothing toward streaks.
- **Shooting it down:** it has 1000 health (`heli_maxhealth`). Bullets do half their damage (`heli_armor_bulletdamage`), checked where the shooter saw it, and any rocket passing within 6 m downs it. The shooter scores 100 and the "Destroyed Attack Chopper" medal; both sides hear the call, and its wreck spins down. Bots carrying a launcher go after it as they do UAVs (Black Ops 2's `threat_requires_rocket` covers helicopters).
- **Minimap:** it never shows on the enemy's minimap, as the Stealth Chopper's name says. No streak draws there yet.
- **Calls:** "ATTACK CHOPPER INBOUND" and "ENEMY ATTACK CHOPPER INBOUND" when it is called.
- **Roofed maps** leave it out.

`attack_chopper_test` plays it through on a server.

## Sentry Gun

The Sentry Gun follows Black Ops 2's (`_turret_killstreak.gsc`). The rules are in `rust/sim/src/turret.rs`, the server's side in `update_turrets`, and the client's in `godot/scripts/net/sentry_view.gd`. The model is the training sentry's, built in Blender (`sentry_base`, `sentry_head`, `sentry_eye`).

![A Sentry Gun set down](media/sentry-gun.jpg)

- **Setting it down:** calling it in sets it down a step (1.2 m) in front of the caller, facing the way they face. It needs clear floor near the caller's feet with nothing in between, or the streak stays held. Black Ops 2 has the player carry it and choose the spot. We set it down where the player stands facing, as the Care Package lands where they aim.
- **Limits:** four a side and eight in all, Black Ops 2's "turret" rule. A fifth on one side stays held.
- **Arming:** it waits 1.6 s before it looks for targets (`turretinitdelay`).
- **Firing:** it fires on the nearest enemy it can see in its field of fire: a half circle in front of the way it was set down, out to 40 m, up to about 50° above or below level. It slews onto them, takes a third of a second to spin up, then fires 600 rounds a minute at 25 damage, with a little spread. Rounds stop at walls and hurt the first enemy they meet. With no one to fire on, it turns back to watch its front. The arc, reach, rate, and damage are estimates: Black Ops 2 keeps them in the turret's weapon file, which its scripts do not carry.
- **Its kills:** they are its owner's, shown as "Sentry Gun" in the feed. They score as usual but add nothing to the owner's streak count. It keeps firing after its owner dies, and goes if they leave the game.
- **Lifetime:** 240 s (`level.auto_turret_timeout`), then it goes.
- **Destroying it:** only enemies can. It has 650 health:
  - bullets do 60% of their damage;
  - blasts do double, so two RL-4 rockets (200 each) destroy it, and so does a frag thrown close;
  - the knife does nothing.

  Shots meet it in a sphere around its gun. Destroying one pays 100 points and the "Destroyed Sentry Gun" medal, and both sides hear "ENEMY SENTRY GUN DESTROYED" or "SENTRY GUN DESTROYED". It blows apart.
- **Bots** set theirs down where they stand. With no enemy player in sight, they shoot an armed enemy Sentry Gun they can see within 45 m with whatever they hold. Black Ops 2's bots count an active one as a threat.
- **Sounds:** none of its own yet. It fires with the AK's shot pitched up, as the training sentry does.

`sentry_gun_test` plays it through on a server.

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
- **Stealth Chopper** (`_helicopter.gsc`): 1100, the player picks the point on the map, `heli_protect_time` 60, `heli_protect_pos_time` 12, spots 20% to 100% of `heli_protect_radius` 2,000 from the point, `heli_maxhealth` 1000, bullets × `heli_armor_bulletdamage` 0.5, a rocket downs it, `heli_targeting_delay` 0.6, `heli_turret_spinup_delay` 0.7, `heli_turretclipsize` 20, `heli_turretreloadtime` 1.5, `heli_visual_range` 3,500, `heli_target_spawnprotection` 5, it goes for the nearest target, one a team (`_killstreakrules.gsc`). Blind Eye hides players from it (`cantargetplayerwithspecialty`).
- **Hellstorm Missile** (`_remotemissile.gsc`): 700, launched 18,000 units up and 7,000 back from a point 1,500 units ahead, a 9.95 s flight, aim boosts until the split, fire splits it into a bomblet for each enemy within 600 units of the aim point (not Cold-Blooded players) then random ones 200 to 550 units out up to nine, plus one straight on, a frame apart; the camera stays a second after the first bomblet lands. Bots fire it 1.5 s in.
- **Sentry Gun** (`_turret_killstreak.gsc`, `_killstreakrules.gsc`): 800, 650 health, bullets at 0.6 and explosives at 2 (a launcher counts 200), the knife nothing, 240 s placed, 1.6 s before it arms, four a team and eight in all. Bots treat an active one as a threat.
- **UAV and Counter-UAV in the sky** (`_spyplane.gsc`, Black Ops 2's scripts as the Plutonium project publishes them): 700 health, bullets do their damage, any rocket downs one, the UAV rig turns clockwise once a minute and the Counter-UAV's the other way every 40 s, a downed one ends at once. Bots fire launchers only at aircraft, never at one more than 70° up (`_bot_combat.gsc`).
