# Create-a-Class

Players spawn with a class: two weapons with their attachments, grenades, and wildcards. The rules follow Black Ops 2's Pick 10 (kept in Black Ops 3 and 4). They live in `rust/sim/src/class.rs`.

## Pick 10

A class has 10 points. Every item costs one: a weapon, each attachment, each grenade, each wildcard. An empty slot frees its point.

| Slot | Limit | Raised by |
| --- | --- | --- |
| Primary weapon | one, any primary | |
| Primary attachments | two | Third Attachment: three |
| Secondary weapon | one, any secondary | Two Primaries: a primary instead |
| Lethal | one frag | Extra Lethal: two |
| Tactical | two of one kind (flashbang or smoke) | |

- Every combination is its own weapon-table variant, built at load by applying each attachment's changes in turn (`m4+reddot+grip`, `weapon/def.rs::variants_of`): up to three attachments, one optic. The first-person model fits every part together (`WeaponView::set_attachments`).
- A weapon takes only the attachments its file lists (`attachments:` in `godot/data/weapons/*.ron`), and one optic at most (red dot or ACOG).
- Wildcards cost a point each. Their names describe what they do; Call of Duty's names are not used.
- A primary carried with Two Primaries takes up to two attachments, as in Black Ops 2.

Deliberate differences from Black Ops 2:

- A class must carry at least one gun. There is no knife-only weapon to fall back on.
- No perks yet, so no perk slots and no Perk Greed wildcards.
- Secondary Gunfighter and secondary attachments wait for a pistol that takes attachments. Tactician (a tactical in the lethal slot) waits for a lethal button that can throw a tactical.

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

- Pick 10, its per-item costs, and the wildcards: Activision's Black Ops II support pages ("What is the Pick 10 feature", "What are wildcards") and the Call of Duty wiki's Wildcards page.
- Black Ops 2 slot limits: two primary attachments (three with Primary Gunfighter), one lethal (two with Danger Close), up to two tacticals, a primary in the secondary slot with Overkill.
- Default classes before Create-a-Class unlocks: Black Ops 2 and Black Ops 3 unlock Create-a-Class at level 4.
- Levels: 55 in Black Ops 2 and 3, each needing more XP than the last (Activision support, "Prestige mode in Black Ops III"). Score as XP: Call of Duty 4 through Black Ops; Black Ops 2's match bonus by time and mode (Call of Duty wiki, XP).
- Attachments unlocked by kills with the weapon: Black Ops 2's weapon levels.
