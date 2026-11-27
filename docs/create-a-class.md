# Create-a-Class

Players spawn with a class: two weapons with their attachments, grenades,
perks, and wildcards. The rules follow Black Ops 2's Pick 10 (kept in Black
Ops 3 and 4). They live in `rust/sim/src/class.rs` (slots, wildcards, default
classes), `rust/sim/src/perk.rs` (perks), and `rust/sim/src/progression.rs`
(unlocks). XP, levels, challenges, camos, calling cards, and prestige are in
[progression.md](progression.md); the weapons themselves in
[weapons.md](weapons.md).

## Pick 10

A class has 10 points. Every item costs one: a weapon, each attachment, each
grenade, each perk, each wildcard. An empty slot frees its point.

| Slot | Limit | Raised by |
| --- | --- | --- |
| Primary weapon | one, any primary | |
| Primary attachments | two | Third Attachment: three |
| Secondary weapon | one, any secondary | Two Primaries: a primary instead |
| Lethal | one frag | Extra Lethal: two |
| Tactical | two of one kind (flashbang or smoke) | |
| Perk 1, Perk 2, Perk 3 | one perk of that tier | Extra Perk 1, 2, or 3: two |

- A weapon takes only the attachments its file lists (`attachments:` in
  `godot/data/weapons/*.ron`), and one optic at most (red dot or ACOG). Each
  combination is its own weapon-table variant ([weapons.md](weapons.md)).
- A primary carried with Two Primaries takes up to two attachments, as in
  Black Ops 2. A sidearm takes none.
- Player-facing names describe what an item does; Call of Duty's names are
  kept in the code and here for reference.

### Wildcards

| Wildcard (in game) | Black Ops 2 | Effect | Level |
| --- | --- | --- | --- |
| Third Attachment | Primary Gunfighter | a third attachment on the primary | 19 |
| Two Primaries | Overkill | a primary in the secondary slot | 28 |
| Extra Lethal | Danger Close | a second frag | 10 |
| Extra Perk 1, 2, 3 | Perk 1, 2, 3 Greed | a second perk of that tier | 25, 31, 34 |

Taking a wildcard off drops what only it allowed.

Deliberate differences from Black Ops 2:

- A class must carry at least one gun. There is no knife-only weapon to fall
  back on (the knife is on its own button anyway).
- No Secondary Gunfighter and no sidearm attachments. The BP-9's file lists a
  red dot, suppressor, and extended magazine, but the class rules allow a
  sidearm none, so the editor shows them greyed out.
- No Tactician (a tactical in the lethal slot): the lethal button throws only
  a frag.

## Perks

Black Ops 2's three perk tiers (kept in Black Ops 3). Each tier is a slot
taking one perk for a point; an Extra Perk wildcard (Black Ops 2's Perk Greed)
opens a second perk of that tier. The rules and numbers are in
`rust/sim/src/perk.rs`.

| Tier | Perk (in game) | Effect | Black Ops 2 number | Level |
| --- | --- | --- | --- | --- |
| 1 | Lightweight (Light Load) | moves 7% faster in every stance; no fall damage | `perk_speedMultiplier` 1.07 | 4 |
| 1 | Flak Jacket (Blast Vest) | takes 35% of explosive damage | `perk_flakJacket` 35 | 7 |
| 1 | Hardline (Fast Track) | score counts 20% more toward scorestreaks | +20% (Call of Duty wiki) | 8 |
| 1 | Blind Eye (Cloud Cover) | an enemy Attack Chopper never targets them | `specialty_nottargetedbyairsupport` | 12 |
| 1 | Ghost (Low Profile) | hidden from enemy UAV sweeps while moving faster than 0.5 m/s, planting, or defusing | moving: an estimate | 15 |
| 2 | Toughness (Unshaken) | a hit's aim flinch is a quarter as strong | `perk_damageKickReduction` 0.25 | 4 |
| 2 | Fast Hands (Quick Hands) | switches weapons in half the time; a throw's wind-up and follow-through take 60% | `perk_weapSwitchMultiplier` 0.5, `perk_weapEquipmentUseMultiplier` 0.6 | 5 |
| 2 | Sleight of Hand (Fast Reload) | reloads in half the time | `perk_weapReloadMultiplier` 0.5 | 11 |
| 2 | Scavenger (Scrounger) | refills ammo and grenades from packs dead enemies drop | one magazine, one lethal, one tactical per pack (`perk_scavenger_*`) | 17 |
| 2 | Hard Wired (Jam Proof) | an enemy Counter-UAV does not jam the minimap | | 18 |
| 2 | Cold-Blooded (Unmarked) | an enemy aiming at them sees no red name or crosshair; an enemy Guided Missile neither boxes them nor sends a bomblet at them | `specialty_nokillstreakreticle` | 23 |
| 3 | Dexterity (Climber) | mantles and vaults in 60% of the time, climbs ladders 1.75 x as fast, weapon up after a sprint in half the time | `perk_mantleReduction` 0.4, `perk_fastLadderClimbMultiplier` 1.75; the sprint-out share is an estimate | 4 |
| 3 | Tactical Mask (Flash Guard) | flashbangs blind for half as long | `perk_shellShockReduction` 0.5 | 9 |
| 3 | Dead Silence (Soft Step) | movement 12 dB quieter, heard at a quarter of the range | none published; an estimate | 14 |
| 3 | Awareness (Keen Ears) | enemy movement 6 dB louder, heard 1.5 times as far | none published; an estimate | 20 |
| 3 | Engineer (Field Tech) | sees enemy Sentry Guns and Care Packages through walls | | 26 |

Where each one acts:

- **Predicted perks** (Lightweight, Fast Hands, Sleight of Hand, Dexterity)
  change movement and weapon timing, so they are part of the player's
  simulated state (`PlayerState::perks`), set from the class at spawn and sent
  in every own-state snapshot. The client steps with the same perks the
  server does, so they never mispredict
  (`perks_predict_exactly_what_the_server_runs`). Handling perks run the
  action's own timer faster (`perk::Handling`), so every stage of a reload or
  switch, and its animation, keeps its proportions. A grenade's pin and fuse
  keep real time under Fast Hands; only the arm is quicker.
- **Flak Jacket** scales grenade and rocket damage on the server, except a
  rocket's direct hit. Search and Destroy's charge is not reduced.
- **Toughness** scales the hit flinch on the client that flinches (the flinch
  turns the look, which the server sees through the view angles). A fall's
  jolt is not reduced.
- **Tactical Mask** shortens the blind on the flashed client, and on the
  server for bots.
- **Dead Silence and Awareness** change how movement sounds (`perk::heard`):
  steps, ladder rungs, jumps, landings, slides, and climbs, not reloads. The
  server marks a Dead Silence player as quiet in every view of them
  (`PlayerView::quiet`), and each client applies its own Awareness to enemies
  (`RemotePlayer::set_hearing`). With both, Awareness turns a Dead Silence
  enemy up, but they stay far quieter than anyone else. Dead Silence quiets a
  player's own steps too, as in Black Ops 2. Bots hear enemies within 7 m
  whichever way they face; Dead Silence shrinks that to a quarter, a bot's
  Awareness stretches it by half.
- **Scavenger** (`rust/sim/src/scavenger.rs`): every player who dies leaves an
  ammunition pouch at their feet (a Blender model,
  `tools/gear_models.py` `scavenger_pack`). Only a player with Scavenger sees
  their enemies' pouches (anyone else's without teams), turning slowly on the
  ground, and walking over one (within 1 m) takes it: a magazine's worth of
  reserve for every gun carried and a frag and a tactical back, each up to
  what the class started with. The first Scavenger player to reach a pouch
  takes it. A pouch nobody takes is gone after 30 s (an estimate; at most 32
  lie at once), and a new round or match clears them. The server holds the
  pouches and decides every pickup; clients draw them from events
  (`ScavengerEvent`), and the taker hears the pouch foley and sees the perk's
  icon under the crosshair (`godot/scripts/net/scavenger_view.gd`).
  `scavenger_test.gd` plays it through on a server. Bots with Scavenger go for
  a pouch within 15 m when the gun in hand has less than a magazine in
  reserve, unless their mode has them carrying, planting, or defusing.
- **Hardline, Ghost, and Hard Wired** act on the server where scorestreaks are
  decided ([scorestreaks.md](scorestreaks.md)): Hardline on each player's
  streak count, Ghost on which enemies a UAV's sweep finds, and Hard Wired on
  whose minimap a Counter-UAV jams.
- **Blind Eye** acts on the server: an Attack Chopper's gun passes the player
  by (`chopper::Candidate::ignored`). Black Ops 2's own descriptions name its
  AI air streaks, not the Sentry Gun, so a Sentry Gun still fires on them.
  Its script also lets a Blind Eye player who stays still too long be
  targeted (`perk_nottargetedbyai_graceperiod`); the dvar's value is not in
  the scripts, so that is left out.
- **Cold-Blooded:** every view of the player carries `PlayerView::unmarked`.
  An enemy's crosshair stays white on them and their name does not show, and
  an enemy Guided Missile does not box them or send a bomblet at them. The aim
  reveal it hides is Black Ops 2's too: aiming at any other enemy with no wall
  between turns the crosshair red and shows their name in red over them
  (`NetClient::update_aimed`, `RemotePlayer::set_marked`).
- **Engineer:** a small red marker shows each enemy Sentry Gun and each landed
  Care Package not ours, through walls (`godot/scripts/net/engineer_view.gd`),
  as Black Ops 2's Engineer outlines enemy equipment and scorestreaks.

Left out: Extreme Conditioning (sprint is unlimited, a recorded project
decision against stamina). Parts of other perks whose targets do not exist
here are left out too: Hard Wired's EMP shield, Cold-Blooded's thermal,
sensor, and Target Finder immunity, and Engineer's booby-trapped and rerolled
Care Packages and delayed enemy explosives. Sleight of Hand is not in Black
Ops 2 (its Fast Mag attachment did the job); it takes Black Ops 1's Perk 2
slot here, since there is no Fast Mag.

Bots take a random perk of each tier every life.

In the class menu, Perk 1, Perk 2, and Perk 3 are rows of chips with their
icons (`godot/assets/ui/perks/`, white line icons drawn for this game). The
row's note says what the chosen perks do; a chip's tooltip says what it would
do. Picking another perk in a full slot swaps it in, as Black Ops 2's menu
does. Taking an Extra Perk wildcard off keeps the tier's first perk. The
class card lists each tier's perks with their icons.

![The editor's perk tiers](media/perks-editor.jpg)

## Default classes

Six ready-made classes, one per weapon class, each with two attachments and a
perk of each tier (in-game perk names in brackets):

| Class | Weapon and attachments | Tactical | Perks |
| --- | --- | --- | --- |
| Assault | M4A1, red dot and grip | flashbang | Lightweight, Fast Hands, Dexterity (Light Load, Quick Hands, Climber) |
| Rifleman | AK-47, red dot and extended magazine | flashbang | Flak Jacket, Toughness, Tactical Mask (Blast Vest, Unshaken, Flash Guard) |
| Close Quarters | PDW-9, suppressor and grip | flashbang | Lightweight, Sleight of Hand, Dead Silence (Light Load, Fast Reload, Soft Step) |
| Support | LSW-6, grip and extended magazine | smoke | Flak Jacket, Sleight of Hand, Awareness (Blast Vest, Fast Reload, Keen Ears) |
| Breacher | M870, grip and extended magazine | flashbang | Lightweight, Fast Hands, Dexterity (Light Load, Quick Hands, Climber) |
| Marksman | M40, suppressor and extended magazine | smoke | Flak Jacket, Toughness, Tactical Mask (Blast Vest, Unshaken, Flash Guard) |

Each carries the M1911, one frag, and one tactical: nine points, as Black Ops
2's defaults leave room. A default class always carries its items, whatever
the player's level. A player who has not picked a class spawns with Assault.
Bots take a random default class each life, as Call of Duty's bots use the
default classes, with its primary swapped for any weapon of the same class
(the Marksman class for a sniper or marksman rifle) and its sidearm for any
secondary; one bot in four carries the RL-4 instead (`class::bot_class`).

## The class menu

The match menu (L, or D-pad up) opens on the class list
(`godot/scripts/net/class_menu.gd`): five custom classes, then the defaults,
as in Black Ops 2, with the player's level and the XP to the next above them.
Choosing one equips it from the next spawn; hovering or focusing one shows
what it carries and its points. The death screen's hint opens the same menu.

A custom class opens in the editor: a name (up to 20 characters), then every
item as a chip, grouped by slot, with ten pips for the points spent, and a
camo for each weapon ([progression.md](progression.md)). A chip that would
break a rule is greyed out: the menu asks `NetClient.check_class` about the
class the chip would make, which runs the server's own rules. A locked item
says what it needs ("Level 12", "8 kills"). Changing weapon keeps the
attachments the new one takes; taking a wildcard off drops what only it
allowed. Back leaves the editor without saving. Under the list sit the
scorestreaks, up to three, shared by every class ([scorestreaks.md](scorestreaks.md)).

Classes are saved in `user://classes.cfg` (`godot/scripts/class_store.gd`),
with which one is equipped; signed in, the account keeps them too
([online.md](online.md)). A custom slot starts as a copy of the default in
its place. A saved class that no longer passes the rules (a weapon removed,
say) goes back to its default.

![The class list and the editor](media/create-a-class.jpg)

## Unlocks

Classic Call of Duty unlocks (Call of Duty 4 through Black Ops 3): the player
level opens Create-a-Class and each item, and kills with a weapon open its
attachments (`rust/sim/src/progression.rs`).

- **Create-a-Class opens at level 4**, as in Black Ops 2 and 3. Until then
  the custom classes are locked and players pick a default class.
- **Weapons, tacticals, perks, wildcards, and scorestreaks unlock by level.**
  Each weapon's level is `unlock_level` in its file; the rest are
  `tactical_level`, `perk_level`, `wildcard_level`, and
  `Streak::unlock_level`. Create-a-Class opens with one perk of each tier, as
  Black Ops 2's does.

  | Level | Unlocks |
  | --- | --- |
  | 1 | M4A1, PDW-9, M1911, frag, flashbang; UAV, Counter-UAV |
  | 4 | Create-a-Class, AK-47; Lightweight, Toughness, Dexterity |
  | 5 | Fast Hands |
  | 6 | Smoke grenade |
  | 7 | Flak Jacket; Care Package |
  | 8 | M870; Hardline |
  | 9 | Tactical Mask |
  | 10 | Extra Lethal; Guided Missile |
  | 11 | Sleight of Hand |
  | 12 | BR-3; Blind Eye |
  | 13 | BP-9; Airstrike |
  | 14 | Dead Silence |
  | 15 | Ghost |
  | 16 | LSW-6; Sentry Gun |
  | 17 | Scavenger |
  | 18 | Hard Wired |
  | 19 | Third Attachment, RL-4 |
  | 20 | Awareness |
  | 22 | M40; Attack Chopper |
  | 23 | Cold-Blooded |
  | 25 | Extra Perk 1 |
  | 26 | Engineer |
  | 28 | Two Primaries, CQ-10 |
  | 31 | KR-12, Extra Perk 2 |
  | 34 | SG-12, Extra Perk 3 |
  | 37 | DM-7 |

- **Attachments unlock by kills with that weapon**, as Black Ops 2's weapon
  levels do, in the same order on every weapon: red dot 5, grip 15, extended
  magazine 30, ACOG 50, suppressor 80 (`attachment_kills`).
- **Prestige** sends the level back to 1, so level unlocks lock again
  (Create-a-Class included). Each prestige pays a token that unlocks one item
  for good, and the master's prestige unlocks everything
  ([progression.md](progression.md)).
- A custom class saved with something still locked plays without it: a locked
  weapon gives way to the first unlocked one of its slot, and locked
  attachments, tacticals, perks, and wildcards come off
  (`ProgressStore.unlocked_class`).

![The class list at level 9 (top) and the editor with locked items (bottom)](media/class-unlocks.jpg)

## Server authority

The client sends its whole class (`ClientMessage::Class`). The server checks
it with `Class::validate` and ignores one that breaks a rule; the player keeps
their previous class. A ranked server (`--accounts`, [server.md](server.md))
also checks the player's account (`Earned::check_class`) and ignores a class
with anything still locked; other servers cannot know a player's level and
check only the class rules. A new class applies from the next spawn; in
Search and Destroy it applies at once during a round's freeze. Gun Game
ignores classes, and a custom game's loadout rule (default classes, pistols
only, snipers only) can restrict them ([custom-games.md](custom-games.md)).

The decoder checks only the shape (real weapon ids, known attachment, perk,
and wildcard bits). The rules are the server's to check, so a modified client
gains nothing.

## Call of Duty reference

- Pick 10, its per-item costs, and the wildcards: Activision's Black Ops II
  support pages ("What is the Pick 10 feature", "What are wildcards") and the
  Call of Duty wiki's Wildcards page.
- Black Ops 2 slot limits: two primary attachments (three with Primary
  Gunfighter), one lethal (two with Danger Close), up to two tacticals, a
  primary in the secondary slot with Overkill; five custom class slots.
- Perks: Black Ops 2's tiers (Perk 1: Lightweight, Hardline, Blind Eye, Flak
  Jacket, Ghost; Perk 2: Toughness, Cold-Blooded, Fast Hands, Hard Wired,
  Scavenger; Perk 3: Dexterity, Extreme Conditioning, Engineer, Tactical
  Mask, Dead Silence, Awareness) and the Perk Greed wildcards (Call of Duty
  wiki, Perks and Wildcards pages). Numbers from Black Ops 2's perk variables
  as dumped by players (denkirson.proboards.com, "Black Ops II Perk
  Variables", 2013); the dump does not say which perk each belongs to, and a
  few are matched by name only.
- Default classes before Create-a-Class unlocks: Black Ops 2 and Black Ops 3
  unlock Create-a-Class at level 4.
- Attachments unlocked by kills with the weapon: Black Ops 2's weapon levels.
