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

## Server authority

The client sends its whole class (`ClientMessage::Class`, protocol 45). The server checks it with `Class::validate` and ignores one that breaks a rule; the player keeps their previous class. A new class applies from the next spawn. In Search and Destroy it applies at once during a round's freeze. Gun Game ignores classes.

The decoder checks only the shape (real weapon ids, known attachment and wildcard bits). The rules are the server's to check, so a modified client gains nothing.

## Call of Duty reference

- Pick 10, its per-item costs, and the wildcards: Activision's Black Ops II support pages ("What is the Pick 10 feature", "What are wildcards") and the Call of Duty wiki's Wildcards page.
- Black Ops 2 slot limits: two primary attachments (three with Primary Gunfighter), one lethal (two with Danger Close), up to two tacticals, a primary in the secondary slot with Overkill.
- Default classes before Create-a-Class unlocks: Black Ops 2 and Black Ops 3 unlock Create-a-Class at level 4.
