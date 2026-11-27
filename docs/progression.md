# Progression

Call of Duty's long-term progression, kept to its core: XP and levels. What each level and weapon kill unlocks (weapons, attachments, perks, wildcards, scorestreaks) is in [create-a-class.md](create-a-class.md).

The game keeps progress on the machine, in `user://progress.cfg` (`godot/scripts/progress_store.gd`): the total XP and the kills with each weapon. Keys an older save holds and the game no longer reads (prestige, stats) are ignored, and the next save drops them.

## XP and levels

The rules are in `rust/sim/src/progression.rs`, as classic Call of Duty (Call of Duty 4 through Black Ops 3) has them.

| Rule | Here |
| --- | --- |
| Score is XP | Every point scored in a match is XP: kills, headshot and medal bonuses, objective awards |
| Match bonus | 100 XP a minute played (to the second, at most 60 minutes), half again for a win (`match_bonus`). A player who leaves early keeps their score but gets no bonus |
| Levels | 1 to 55, as in Black Ops 2 and 3. Level 1 to 2 takes 600 XP, and each level 400 more than the last (`xp_to_next`): 604,800 XP to level 55 |
| Create-a-Class | Opens at level 4, as in Black Ops 2 and 3 (`CREATE_A_CLASS_LEVEL`) |
| Weapon kills | Kills with each weapon, by its plain key (every M4A1 variant counts as `m4`), unlock its attachments |

The game works out a match's XP when it ends (`godot/scripts/net/match_xp.gd`) and shows it on the XP card beside the results (`godot/scripts/net/xp_summary.gd`): score, bonus, the level bar, what any level reached unlocks, and how far each weapon used is from its next attachment.

![The XP card beside the results](media/match-xp.jpg)
