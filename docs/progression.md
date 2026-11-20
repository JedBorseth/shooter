# Long-term progression

Black Ops 2's long-term progression, on top of the XP, levels, and unlocks in [create-a-class.md](create-a-class.md):

- career stats;
- challenges that pay XP;
- weapon camos;
- calling cards;
- prestige;
- a combat record.

Where it is kept is unchanged ([online.md](online.md)):

- **Signed in:** the account keeps it, and a ranked server writes it.
- **Signed out:** `user://progress.cfg` keeps it.

## Stats

The server counts each player's stats through a match (`rust/sim/src/stats.rs`, `Recorder`). It reads them from the events it sends: kills, hits, score awards, and scorestreaks earned. Black Ops 2 does the same: its game server keeps every player's stats and per-weapon stats (`_persistence.gsc`), and every challenge is a target on one of those counters.

| Stats | Keys |
| --- | --- |
| Career | `kills`, `deaths`, `headshots`, `shots`, `hits`, `score`, `time` (seconds), `matches`, `wins`, `losses`, `draws`, `longshots`, `revenge`, `doubles`, `triples`, `bloodthirsty` (5 kills without dying), `merciless` (10), `collateral`, `point_blank`, `aircraft` (enemy aircraft shot down) |
| Other kills | `knife.kills`, `frag.kills`, `charge.kills` |
| Per weapon | `w.<weapon>.hs`, `.long`, `.bare` (no attachments), `.noperk` (a class with no perks), `.double`, `.bt`, `.revenge`, `.ohk` (one-hit kills: a full bar in one hit), `.shots`, `.hits`, `.deaths` |
| Objectives | `a.plant`, `a.defuse`, `a.flag_capture`, `a.hp_capture`, `a.dom_capture`, `a.clutch`, and the rest of the objective awards |
| Scorestreaks | `s.<streak>.earned`, `s.<streak>.kills`, `s.<streak>.downed` |
| Modes | `m.<mode>.played`, `m.<mode>.wins` |

Kills with each weapon stay in `weapon_kills`, which attachments unlock by.

How the stats move:

1. The server sends each player their match's stats (`GameEvent::Stats`, protocol 77) every two seconds while they change, and once more just before the match ends.
2. The game adds them to the player's totals with the match's XP (`ProgressStore.add`).
3. On a ranked server, the server reports them to the account itself (`server_progress_add`).

Things the server does not count:

- Bots.
- Zombies. Call of Duty keeps Zombies' stats apart.
- Time outside play: the lobby, the countdown, and the results.

A player who leaves mid-match keeps the stats on a ranked server, which reports what they had. Elsewhere, the game keeps the stats it was last sent, at most two seconds old.

## Challenges

A challenge is a target on a stat (`rust/sim/src/challenge.rs`).

- It completes in the match that takes the stat past the target, and it pays its XP in that match.
- Completion is never stored: it is worked out from the totals. The game, the ranked server, and the account always agree on it, and each challenge pays once.
- The end-of-match card shows the challenges' XP on its own line and lists the challenges completed, with the camo or card each awarded.

### Weapon challenges: the camo ladder

Every weapon has 14 challenges, each earning one camo, in Black Ops 2's order for its class.

| Class | Camos 1 to 9 | Camo 10 | Camos 11 and 12 | Camo 13 | Camo 14 |
| --- | --- | --- | --- | --- | --- |
| Assault rifles, marksman rifles | headshots: 5, 10, 15, 20, 30, 40, 50, 75, 100 | 10 longshots | 150 kills with no attachments; 150 kills with no perks | 20 double kills | 5 kills without dying, 10 times |
| SMGs, LMGs, pistols | headshots, as above | 30 revenge kills | 150; 150 | 20 double kills | 10 times |
| Shotguns | one-hit kills: 5 to 100, as above | 30 revenge kills | 50; 50 | 5 double kills | 10 times |
| Sniper rifles | one-hit kills: 5, 10, 25, 50, 75, 100, 150, 200, 250 | 10 longshots | 50; 50 | 5 double kills | 10 times |
| Launchers | kills: 5, 10, 20, 30, 40, 50, 60, 75, 100 | 10 longshots | 150 kills; 50 kills with no perks | 5 double kills | 5 times |

Past the ladder:

- **Gold:** all 14 of a weapon's challenges done.
- **Mastery:** Gold on every weapon of a class. This is Black Ops 2's Diamond.
- **Nebula:** mastery in every class. This is Black Ops 3's Dark Matter; Black Ops 2 had no camo for every class. Nebula also awards the Nebula calling card.

Deliberate differences from Black Ops 2:

- **Launchers** count kills, where Black Ops 2 counted aircraft destroyed. Aircraft shot down count toward a career challenge instead, with any weapon. With no attachments, every launcher kill is a bare one, so a 150-kill challenge stands in for the attachment challenge.
- **Marksman rifles** are their own class here; Black Ops 2's were assault rifles, so they climb the assault rifle ladder.

### Career, mode, and scorestreak challenges

These are tiered, as Black Ops 2's are, and the last tier of most awards a calling card:

- **Career:** kills, headshots, wins, matches, longshots, revenge, double and triple kills, 5 and 10 kills without dying, knife and frag kills, collaterals, aircraft shot down.
- **Modes:**
  - wins in each mode;
  - flag captures and defends in Domination;
  - zone captures and defends in Hardpoint;
  - captures, returns, and carrier kills in Capture the Flag;
  - plants, defuses, Wire Cutters, Clutches, and Aces in Search and Destroy.
- **Scorestreaks:** times each streak is earned, and Airstrike kills.

### XP

Black Ops 2's challenges pay 250 to 5,000 XP each. A weapon's whole ladder comes to about 1.4% of the XP to level 55 there (Call of Duty wiki, "Challenges/Call of Duty: Black Ops II"; per-tier figures are not published, so this is approximate). Ours keep that share of our shorter curve:

| Challenges | XP |
| --- | --- |
| Camos 1 to 9 | 250, 250, 500, 500, 500, 750, 750, 1,000, 1,000 |
| Camos 10 to 14 | 500 each |
| Gold | 1,000 |
| A weapon's whole ladder | 9,000 of the 604,800 to level 55 |
| Mastery and Nebula | none, as Black Ops 2's Diamond paid none |
| Career, mode, and scorestreak tiers | 500 to 5,000, as Black Ops 2's |

## Camos

Each weapon has 14 ladder camos, Gold, Mastery, and Nebula, our own designs (`rust/sim/src/camo.rs`):

| Kind | Camos |
| --- | --- |
| Ladder, 1 to 14 | Gravel, Dune, Timber, Tundra, Cocoa, Riptide, Ember, Static, Scales, Weave, Bloom, Ink, Hazard, Circuit |
| Past the ladder | Gold, Prism (mastery), Nebula (final) |

![Every camo pattern: the ladder, Gold, Prism (shown as the data its shader reads), and Nebula](media/camo-patterns.jpg)

- **Patterns.** `tools/build_camos.py` paints each pattern as a seamless 512 px texture (`godot/assets/camos/`). It needs only numpy and is seeded, so it rebuilds the same files.
- **Shader** (`godot/shaders/weapon_camo.gdshaderinc`):
  - It projects the pattern onto the gun from three sides in the gun's own space, so it needs no UVs.
  - It paints the pattern over the coated parts: whatever the weapon's ORM map does not mark as bare metal. This is the same finish `tools/weapon_finish.py` paints.
  - Edges worn to bare metal stay bare, and the finish's grime and occlusion darken the paint, as Call of Duty's camos sit under the weapon's wear.
  - **Gold** is polished metal.
  - **Prism** has facets whose hue turns with the view.
  - **Nebula** has slowly drifting clouds with glinting stars.
- **No extra draw calls.**
  - In first person, a camo is a few parameters on the material each surface already has (`viewmodel.gdshader`).
  - In third person and killcams, the weapon's shared material is swapped for one with the camo painted in (`weapon_camo.gdshader`). It is cached per material and camo, so every M4A1 in Gold shares one.
  - `camo_draws_test` checks that the draw count holds.
- **Create-a-Class.** Each weapon of a class takes a camo (`primary_camo`, `secondary_camo`), chosen from the camos that weapon has earned. A note gives how many more its challenges hold. As in Black Ops 2, a camo costs no point. Changing the weapon resets its camo.
- **Others see it.**
  - The game sends its cosmetics with its class (`ClientMessage::Cosmetics`): the prestige, the calling card, and the camo on each weapon. The server passes them to everyone (`GameEvent::Cosmetics`, protocol 82).
  - Other players' third-person weapons wear the camo.
  - In a killcam, the first-person weapon wears the killer's camo; it goes back to ours afterwards.
  - A ranked server takes the prestige from the account and drops any card or camo it has not unlocked (`Cosmetics::checked`). Other servers pass on what the game sends.

![The M4A1 in Gold, in first person](media/camo-gold-first-person.jpg)

![The AK-47 in Dune and in Prism, inspected](media/camo-inspect.jpg)

![Another player's M4A1 in Gold and in Dune](media/camo-third-person.jpg)

![Create-a-Class: the M4A1's earned camos, with their patterns](media/camo-picker.jpg)

## Prestige

Black Ops 2's prestige (`rust/sim/src/progression.rs`):

- **When.** At level 55, the player may enter the next prestige from the career screen's Prestige tab, after confirming.
- **What resets.** The level goes back to 1. Everything a level unlocks locks again until it is reached, Create-a-Class (level 4) included, as in Black Ops 2.
- **What it earns.**
  - The next prestige's emblem and calling card.
  - A permanent unlock token. A token unlocks one weapon, perk, wildcard, tactical, or scorestreak for good: it stays unlocked through every later prestige.
- **What is kept.** Kills with each weapon (its attachments), stats, challenges, camos, calling cards, the combat record, and everything unlocked with a token.
- **How many.** Ten prestiges, then **Master**, the last.
  - At Master every item unlocks for good, as Black Ops 2's Prestige Master does, and no token comes with it.
  - The level keeps counting XP to 55 and stays there.
- **Where it is done.**
  - Signed in, the account does it (`progress_prestige`, `progress_unlock` in `progress.lua`). It checks the level itself, so a game cannot prestige early.
  - Signed out, the game does it (`ProgressStore.prestige_up`).
  - Ranked servers check classes against the account's prestige and permanent unlocks.
  - Prestige earned before accounts comes along with the one-time import, with no more tokens than prestiges.
- **Emblems** (our own designs; `godot/scripts/ui/prestige_emblem.gd`, drawn as vector shapes).
  - A hexagonal badge whose metal climbs with the prestige: steel for 1 to 3, bronze 4 to 6, gold 7 to 9 (one to three chevrons in each band), platinum with a star for 10, and crimson with a ringed star for Master.
  - It shows beside the name on the scoreboard and in the kill feed, from the cosmetics each player shows (#368), and on the career screen.

![The Prestige tab: every emblem, entering the next prestige, and spending a token](media/prestige-tab.jpg)

![Prestige emblems beside names on the scoreboard and in the kill feed](media/prestige-scoreboard.jpg)

## Career screen

**Career**, beside Options on the title screen, opens the player's challenges and combat record (`godot/scripts/ui/career_menu.gd`). Black Ops 2 keeps these in the Barracks.

- **Challenges.** Four categories: Weapons, Career, Modes, and Scorestreaks.
  - The list on the left picks a weapon (grouped by class, with Mastery last) or a mode, each with how many of its challenges are done.
  - The panel lists that section's challenges: name, what to do, progress ("12 / 20", or COMPLETE), what it awards (a camo or a calling card), and its XP.
- **Combat record.** Career totals, as Call of Duty's combat record shows them:
  - K/D;
  - win rate;
  - accuracy (hits over shots fired);
  - time played in matches;
  - score per minute;
  - headshots.

  Below them is a line for each weapon: kills, deaths while holding it, K/D, headshots, accuracy, and camos earned, most kills first.

![The career screen's weapon challenges: the M4A1's camo ladder with its progress and rewards](media/career-weapons.jpg)

![The combat record: career totals and each weapon's line](media/career-record.jpg)

## Account and caps

The account stores `stats` beside `xp` and `weapon_kills` (`docker/online/nakama/modules/progress.lua`). A match's report already includes its challenges' XP: the game or the ranked server works it out from the totals, and the account only adds.

| Report | Caps |
| --- | --- |
| Self-reported (`progress_match`) | XP and kills, as before; each stat at most 600 a minute since the last award (shots fired run fastest); `time` at most the time itself |
| Ranked server | 100,000 per stat |
| Kept on the account | at most 1,024 stat keys, each `a-z0-9_.` and up to 32 bytes |

## Call of Duty reference

- **Career screen:** Black Ops 2's Barracks holds the challenges (by weapon, career, mode, and scorestreak, each with its progress and reward) and the combat record (K/D, win/loss, accuracy, time played, and per-weapon kills, deaths, headshots, and accuracy).
- **Stats and challenges:** Black Ops 2's challenge tables are targets on its player and weapon stats, kept by the game server. A challenge completes, and its XP is paid, in the match that crosses it.
- **The camo ladder:**
  - The numbers for each class follow games.gg, "Black Ops 2: All Weapon Camos & Challenges", and gamerant.com, "Black Ops 2 camo challenges guide": the headshot ladder 5 to 100; then longshots or revenge kills, 150 kills with no attachments, 150 with no perks, 20 double kills, and 10 Bloodthirsty medals.
  - Shotguns and snipers climb one-hit and one-shot kills instead.
  - Gold is all of a weapon's camos.
  - Diamond is Gold on every weapon of a class (Call of Duty wiki, "Diamond Camouflage").
  - Dark Matter is Black Ops 3's camo for every class.
- **Camos:** a camo is chosen per weapon in Create-a-Class, costs no point, sits under the weapon's wear, and shows for other players and in killcams, as Black Ops 2's do.
- **Prestige:** Black Ops 2 has ten prestiges and Prestige Master, reached at level 55. Prestiging resets the level and what it unlocks, and earns the prestige's emblem and an unlock token for one permanent unlock; at Master everything unlocks (Activision support, "Achieving Prestige Master in Call of Duty: Black Ops II"; boostmatch.gg, "Black Ops 2 Prestige System"). Camos, challenges, and the combat record carry over. The prestige icon shows beside the name in lobbies, on the scoreboard, and in the kill feed.
  - Deliberate differences: Black Ops 2 also offered a stat reset or an extra class slot for a token; here a token is only a permanent unlock. Weapon kills (attachments) are kept, so attachments need not be earned again.
- **Names and designs** are our own. None of Black Ops 2's camo, challenge, or calling card names are used.
