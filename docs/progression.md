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

## Account and caps

The account stores `stats` beside `xp` and `weapon_kills` (`docker/online/nakama/modules/progress.lua`). A match's report already includes its challenges' XP: the game or the ranked server works it out from the totals, and the account only adds.

| Report | Caps |
| --- | --- |
| Self-reported (`progress_match`) | XP and kills, as before; each stat at most 600 a minute since the last award (shots fired run fastest); `time` at most the time itself |
| Ranked server | 100,000 per stat |
| Kept on the account | at most 1,024 stat keys, each `a-z0-9_.` and up to 32 bytes |

## Call of Duty reference

- **Stats and challenges:** Black Ops 2's challenge tables are targets on its player and weapon stats, kept by the game server. A challenge completes, and its XP is paid, in the match that crosses it.
- **The camo ladder:**
  - The numbers for each class follow games.gg, "Black Ops 2: All Weapon Camos & Challenges", and gamerant.com, "Black Ops 2 camo challenges guide": the headshot ladder 5 to 100; then longshots or revenge kills, 150 kills with no attachments, 150 with no perks, 20 double kills, and 10 Bloodthirsty medals.
  - Shotguns and snipers climb one-hit and one-shot kills instead.
  - Gold is all of a weapon's camos.
  - Diamond is Gold on every weapon of a class (Call of Duty wiki, "Diamond Camouflage").
  - Dark Matter is Black Ops 3's camo for every class.
- **Names and designs** are our own. None of Black Ops 2's camo, challenge, or calling card names are used.
