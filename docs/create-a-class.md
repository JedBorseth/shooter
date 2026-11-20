# Create-a-Class

Players spawn with a class: two weapons with their attachments, grenades, perks, and wildcards. The rules follow Black Ops 2's Pick 10 (kept in Black Ops 3 and 4). They live in `rust/sim/src/class.rs`.

## Pick 10

A class has 10 points. Every item costs one: a weapon, each attachment, each grenade, each wildcard. An empty slot frees its point.

| Slot | Limit | Raised by |
| --- | --- | --- |
| Primary weapon | one, any primary | |
| Primary attachments | two | Third Attachment: three |
| Secondary weapon | one, any secondary | Two Primaries: a primary instead |
| Lethal | one frag | Extra Lethal: two |
| Tactical | two of one kind (flashbang or smoke) | |
| Perk 1, Perk 2, Perk 3 | one perk of that tier | Extra Perk 1, 2, or 3: two |

- Every combination is its own weapon-table variant, built at load by applying each attachment's changes in turn (`m4+reddot+grip`, `weapon/def.rs::variants_of`): up to three attachments, one optic. The first-person model fits every part together (`WeaponView::set_attachments`).
- A weapon takes only the attachments its file lists (`attachments:` in `godot/data/weapons/*.ron`), and one optic at most (red dot or ACOG).
- Wildcards cost a point each. Their names describe what they do; Call of Duty's names are not used.
- A primary carried with Two Primaries takes up to two attachments, as in Black Ops 2.

Deliberate differences from Black Ops 2:

- A class must carry at least one gun. There is no knife-only weapon to fall back on.
- Secondary Gunfighter and secondary attachments wait for a pistol that takes attachments. Tactician (a tactical in the lethal slot) waits for a lethal button that can throw a tactical.

## Perks

Black Ops 2's three perk tiers (kept in Black Ops 3). Each tier is a slot taking one perk for a point; a Perk Greed wildcard (here "Extra Perk 1/2/3", a point of its own) opens a second perk of that tier, as in Black Ops 2. The rules and numbers are in `rust/sim/src/perk.rs`.

Perk names in the game are plain descriptions, as the wildcards' are; Call of Duty's names are kept here and in the code for reference.

| Tier | Perk (in game) | Effect | Black Ops 2 number |
| --- | --- | --- | --- |
| 1 | Lightweight (Light Load) | moves 7% faster in every stance; no fall damage | `perk_speedMultiplier` 1.07 |
| 1 | Flak Jacket (Blast Vest) | takes 35% of grenade damage | `perk_flakJacket` 35 |
| 2 | Toughness (Unshaken) | a hit's aim flinch is a quarter as strong | flinch to 25% (`perk_damageKickReduction` 0.25) |
| 2 | Fast Hands (Quick Hands) | switches weapons in half the time; a throw's wind-up and follow-through take 60% | `perk_weapSwitchMultiplier` 0.5, `perk_weapEquipmentUseMultiplier` 0.6 |
| 2 | Scavenger (Scrounger) | refills ammo and grenades from packs dead enemies drop | one magazine, one lethal, one tactical per pack (`perk_scavenger_*`) |
| 2 | Sleight of Hand (Fast Reload) | reloads in half the time | `perk_weapReloadMultiplier` 0.5 |
| 3 | Dexterity (Climber) | mantles and vaults in 60% of the time, climbs ladders 75% faster, weapon up after a sprint in half the time | `perk_mantleReduction` 0.4, `perk_fastLadderClimbMultiplier` 1.75; the sprint-out share is an estimate |
| 3 | Dead Silence (Soft Step) | movement 12 dB quieter, heard at a quarter of the range | none published; an estimate |
| 3 | Awareness (Keen Ears) | enemy movement 6 dB louder, heard 1.5 times as far | none published; an estimate |
| 3 | Tactical Mask (Flash Guard) | flashbangs blind for half as long | `perk_shellShockReduction` 0.5 |

How each one is decided:

- **Predicted perks** (Lightweight, Fast Hands, Sleight of Hand, Dexterity) change movement and weapon timing, so they are part of the player's simulated state (`PlayerState::perks`), set from the class at spawn and sent in every own-state snapshot (protocol 54). The client steps with the same perks the server does, so they never mispredict (`perks_predict_exactly_what_the_server_runs`); a client that predicted without them takes them from the first correction. Handling perks run the action's own timer faster (`perk::Handling`), so every stage of a reload or switch, and its animation, keeps its proportions. A grenade's pin and fuse keep real time under Fast Hands; only the arm is quicker.
- **Flak Jacket** scales grenade damage on the server. Search and Destroy's charge is not reduced.
- **Toughness** scales the hit flinch on the client that flinches (the flinch turns the look, which the server sees through the view angles). A fall's jolt is not reduced.
- **Tactical Mask** shortens the blind on the flashed client, and on the server for bots.
- **Dead Silence and Awareness** change how movement sounds (`perk::heard`): steps, ladder rungs, jumps, landings, slides, and climbs, not reloads. The server marks a Dead Silence player as quiet in every view of them (`PlayerView::quiet`, protocol 55), and each client applies its own Awareness to enemies (`RemotePlayer::set_hearing`). Both together: Awareness turns a Dead Silence enemy up, but they stay far quieter than anyone else. Dead Silence quiets a player's own steps too, as in Black Ops 2. Bots hear enemies within 7 m whichever way they face; Dead Silence shrinks that to a quarter, a bot's Awareness stretches it by half.
- **Scavenger** (`rust/sim/src/scavenger.rs`): every player who dies, outside Zombies, leaves an ammunition pouch at their feet (a Blender model, `tools/gear_models.py` `scavenger_pack`). Only a player with Scavenger sees their enemies' pouches (anyone else's without teams), turning slowly on the ground, and walking over one (within 1 m) takes it: a magazine's worth of reserve for every gun carried and a frag and a tactical back, each up to what the class started with. The first Scavenger player to reach a pouch takes it; nobody else's is touched. A pouch nobody takes is gone after 30 s (an estimate), and a new round or match clears them. The server holds the pouches and decides every pickup; clients draw them from events (`ScavengerEvent`, protocol 56), and the taker hears the pouch foley and sees the perk's icon under the crosshair (`godot/scripts/net/scavenger_view.gd`). `scavenger_test` plays it through on a server. Bots with Scavenger go for a pouch within 15 m when the gun in hand has less than a magazine in reserve, unless their mode has them carrying, planting, or defusing.

Left out, because their target does not exist here: Hardline (scorestreaks), Blind Eye and Ghost (UAV and air support), Cold-Blooded (AI streaks and thermal), Hard Wired (counter-UAV and EMP), Engineer (enemy equipment and care packages), and Extreme Conditioning (sprint is unlimited, a recorded project decision against stamina). Sleight of Hand is not in Black Ops 2 (its Fast Mag attachment did the job); it takes Black Ops 1's Perk 2 slot here, since there is no Fast Mag.

Bots take one random perk of each tier every life.

## Default classes

Six ready-made classes, one per weapon class, each with two attachments: Assault (M4A1, red dot and grip), Rifleman (AK-47, red dot and extended magazine), Close Quarters (PDW-9, suppressor and grip), Support (LSW-6, grip and extended magazine), Breacher (M870, grip and extended magazine), Marksman (M40, suppressor and extended magazine). Each carries the M1911, a frag, and one tactical. A player who has not picked a class spawns with Assault. Bots take a random default class each life, as Call of Duty's bots use the default classes.

## The class menu

The match menu (L, or D-pad up) opens on the class list (`godot/scripts/net/class_menu.gd`): five custom classes, then the defaults, as in Black Ops 2. Choosing one equips it from the next spawn; hovering or focusing one shows what it carries and its points. The death screen's hint opens the same menu. A custom class opens in the editor: every item is a chip, grouped by slot, with ten pips for the points spent. A chip that would break a rule is greyed out; the menu asks `NetClient.check_class` about the class the chip would make, which runs the server's own rules. Changing weapon keeps the attachments the new one takes; taking a wildcard off drops what only it allowed. Back leaves the editor without saving.

Classes are saved in `user://classes.cfg` (`godot/scripts/class_store.gd`), with which one is equipped. A custom slot starts as a copy of the default in its place. A saved class that no longer passes the rules (a weapon removed, say) goes back to its default.

![The class list and the editor](media/create-a-class.jpg)

## Progression

XP, levels, and unlocks follow classic Call of Duty (Call of Duty 4 through Black Ops 3). The rules are in `rust/sim/src/progression.rs`; the player's progress is kept in `user://progress.cfg` (`godot/scripts/progress_store.gd`), on this machine until accounts exist (Phase 5).

- **Score is XP.** Every point scored in a match counts: a kill's 100, headshot and multi-kill bonuses, objective awards. At the end of a match a bonus pays 100 XP a minute played, and half again for a win (`match_bonus`). Black Ops 2 based its match bonus on time and mode, not kills; Modern Warfare (2019) onward pays more for a win. A player who leaves early keeps the score and kills made so far but gets no bonus (`godot/scripts/net/match_xp.gd`).
- **Levels 1 to 55**, as in Black Ops 2 and 3. Each level needs 400 XP more than the last, starting at 600 (`xp_to_next`). Level 4 takes about one match against bots; level 55 takes 604,800 XP. Call of Duty's curve has the same shape; the numbers fit our shorter matches.
- **Create-a-Class opens at level 4**, as in Black Ops 2 and 3. Until then the custom classes are locked and players pick a default class. Default classes always carry their items, whatever the level.
- **Weapons, tacticals, and wildcards unlock by level.** Each weapon's level is `unlock_level` in its file.

  | Level | Unlocks |
  | --- | --- |
  | 1 | M4A1, PDW-9, M1911, frag, flashbang |
  | 4 | Create-a-Class, AK-47 |
  | 6 | Smoke grenade |
  | 8 | M870 |
  | 10 | Extra Lethal |
  | 12 | BR-3 |
  | 16 | LSW-6 |
  | 19 | Third Attachment |
  | 22 | M40 |
  | 28 | Two Primaries |

  Perks: Create-a-Class opens with one of each tier, as in Black Ops 2 (Lightweight, Toughness, Dexterity at 4), then Fast Hands 5, Flak Jacket 7, Tactical Mask 9, Sleight of Hand 11, Dead Silence 14, Scavenger 17, Awareness 20. Extra Perk 1 at 25, Extra Perk 2 at 31, Extra Perk 3 at 34 (`perk_level`, `wildcard_level`).

- **Attachments unlock by kills with that weapon**, as Black Ops 2's weapon levels do: red dot 5, grip 15, extended magazine 30, ACOG 50, suppressor 80 (`attachment_kills`).
- The editor shows what each locked item needs ("Level 12", "8 kills"). A custom class saved with something still locked plays without it: a locked weapon gives way to the first unlocked one of its slot (`ProgressStore.unlocked_class`).
- The class list shows the level and the XP to the next.

### End of match

The results screen shows the XP the match paid on a card beside the scoreboard (`godot/scripts/net/xp_summary.gd`), as Call of Duty's after-match screen does:

- the score, the match bonus (marked when it was a win), and the total;
- the level bar filling from where the match started to where it ended, with LEVEL UP when it crossed one;
- what the new levels unlocked;
- for each weapon used, its kills this match and how far its next attachment is ("Grip in 8").

![The XP card beside the results](media/match-xp.jpg)

The server cannot check a level without accounts, so it checks only the class rules. Unlocks are the client's to enforce until Phase 5.

![The class list at level 9 (top) and the editor with locked items (bottom)](media/class-unlocks.jpg)

## Server authority

The client sends its whole class (`ClientMessage::Class`, protocol 45). The server checks it with `Class::validate` and ignores one that breaks a rule; the player keeps their previous class. A new class applies from the next spawn. In Search and Destroy it applies at once during a round's freeze. Gun Game ignores classes.

The decoder checks only the shape (real weapon ids, known attachment and wildcard bits). The rules are the server's to check, so a modified client gains nothing.

## Call of Duty reference

- Perks: Black Ops 2's tiers (Perk 1: Lightweight, Hardline, Blind Eye, Flak Jacket, Ghost; Perk 2: Toughness, Cold-Blooded, Fast Hands, Hard Wired, Scavenger; Perk 3: Dexterity, Extreme Conditioning, Engineer, Tactical Mask, Dead Silence, Awareness) and the Perk Greed wildcards (Call of Duty wiki, Perks and Wildcards pages). Numbers from Black Ops 2's perk variables as dumped by players (denkirson.proboards.com, "Black Ops II Perk Variables", 2013); the dump does not say which perk each belongs to, and a few are matched by name only.

- Pick 10, its per-item costs, and the wildcards: Activision's Black Ops II support pages ("What is the Pick 10 feature", "What are wildcards") and the Call of Duty wiki's Wildcards page.
- Black Ops 2 slot limits: two primary attachments (three with Primary Gunfighter), one lethal (two with Danger Close), up to two tacticals, a primary in the secondary slot with Overkill.
- Default classes before Create-a-Class unlocks: Black Ops 2 and Black Ops 3 unlock Create-a-Class at level 4.
- Levels: 55 in Black Ops 2 and 3, each needing more XP than the last (Activision support, "Prestige mode in Black Ops III"). Score as XP: Call of Duty 4 through Black Ops; Black Ops 2's match bonus by time and mode (Call of Duty wiki, XP).
- Attachments unlocked by kills with the weapon: Black Ops 2's weapon levels.
