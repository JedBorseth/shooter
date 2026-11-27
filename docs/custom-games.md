# Custom games

A host can change a match's rules before it starts, as in Black Ops 2's Custom Games. The rules live in `rust/sim/src/custom.rs` (`custom::Settings`). The server takes them on the command line, applies them, and sends them to every player in the welcome. Prediction (health and its regeneration) and the HUD follow the same rules.

Ranked servers (`--accounts`) ignore custom settings: ranked play always uses the default rules.

## Settings

Each setting follows a Black Ops 2 game type setting (the `getgametypesetting` key in its `_globallogic.gsc` is in brackets).

| Setting | Choices (default first) | What it does |
| --- | --- | --- |
| Score limit (`scoreLimit`) | the mode's, or any number | Kills, captures, or points to win. Search and Destroy uses rounds instead. |
| Time limit (`timeLimit`) | the server's, or 0 to 120 minutes; 0 is no clock | Search and Destroy has no match clock. |
| Rounds (`roundWinLimit`) | the server's, or 1 to 20 | Search and Destroy's rounds to win. |
| Respawn delay (`playerRespawnDelay`) | the mode's (3 s, 5 s in Capture the Flag), or 0 to 30 s (the menu offers 0, 3, 5, and 10) | How long a dead player waits. A killcam still holds the respawn until it ends or is skipped ([killcam.md](killcam.md)). |
| Health (`playerMaxHealth`) | 100%, 30%, 50%, 150%, 200% (1% to 250% in the text form) | Health a life starts with. Weapon damage does not change, so 30% means almost every hit kills. |
| Health regeneration (`playerHealthRegenTime`) | Normal, Slow, Fast, Off | Slow waits twice as long and refills at half the rate; Fast is the opposite. The refill rate scales with health, so a full refill takes as long at any health. |
| Friendly fire (`friendlyfiretype`) | Off, On, Reflect, Shared | On: teammates take full damage. Reflect: the attacker takes half instead. Shared: each takes half. Half is Black Ops 2's: `int(damage * 0.5)`, at least 1. |
| Killcam (`allowKillcam`) | On, Off | Off: no killcam on death and no final or round-winning killcam. |
| Radar (`forceRadar`) | Normal, Constant, Off | Constant: a UAV for every side that never goes down, sweeping every enemy onto the minimap every 3 s (Ghost still hides a moving player). Off: no minimap, and the server sends no dots. |
| Scorestreaks (`custom_killstreak_mode`) | On, Off | Off: points earn no scorestreaks. |
| Loadouts (`disableCAC` and party restrictions) | Any, Default classes, Pistols only, Snipers only | Which classes players may use. |
| Hardcore (`hardcoreMode`) | Off, On | No HUD. Turning it on sets health to 30% and friendly fire to On, Black Ops 2's hardcore rules; either can still be changed. |

## Setting them up

The local match setup (the title screen's mode routes) shows the custom rules in one line above Start, with a **Game settings** button. It opens a page in the same card: a preset list, one chip row per rule, and a name to save the rules under as a preset (`godot/scripts/ui/custom_settings_page.gd`). The built-in presets are Standard, Hardcore, Pistols only, Snipers only, and No streaks; saved presets follow them, up to twelve, in `user://custom_games.cfg` with the rules used last (`godot/scripts/custom_presets.gd`). Turning Hardcore on sets health to 30% and friendly fire to On; either can be changed after. Free Roam has no custom rules.

![Game settings in the match setup, on the Hardcore preset](media/custom-game-settings.jpg)

![The match setup with custom rules summed up above Start](media/custom-match-setup.jpg)

## On the HUD

![Hardcore's bare HUD: no crosshair, health, ammo, minimap, or score](media/hardcore-hud.jpg)

- **Hardcore:** no crosshair (hit markers still show), no health, no ammo or weapon panel, no minimap, and no score block; the scoreboard still shows the score. Prompts, the death card, the kill feed, and scorestreak calls stay. This follows Black Ops 2's `hud_hardcore` match flag.
- **Radar off:** no minimap.
- **Scorestreaks off:** no scorestreak panel.

The client reads `hardcore`, `radar_off`, and `streaks_off` from `NetClient.match_info()`.

## Friendly fire

With friendly fire on, bullets, the knife, grenades, and every other blast (launchers, scorestreaks, props) hurt teammates too, and bullets stop at them instead of passing through. What a hit on a teammate does depends on the setting (`FriendlyFire::split`):

- **On:** the teammate takes it all.
- **Reflect:** the teammate takes nothing, and the attacker takes half (at least 1).
- **Shared:** each takes half (at least 1).

A teamkill shows in the kill feed and counts as the victim's death, but scores nothing: no kill points, no team score, no streak count, and no objective bonus. Death by reflected damage is a suicide. Black Ops 2 also punishes repeat teamkillers (`teamKillPunishCount`: a respawn delay, then a kick); that is not built yet (see [TODO.md](../TODO.md)).

## Loadout rules

The server applies the rule to the class each life spawns with (`Loadouts::restrict`), players' and bots' alike; equipment and perks stay.

- **Default classes:** a custom class becomes the first default class.
- **Pistols only:** no primary; the class's pistol, or the M1911.
- **Snipers only:** the class's sniper rifle, or the M40; the secondary only if it is a pistol.

## Bots per team

In team modes the match setup picks each team's size and its bots' difficulty: **Allies** (your team, you included), **Ally bots**, **Enemies**, and **Enemy bots**. This follows Black Ops 2's Combat Training and custom games, which set friendly and enemy bots apart. The server keeps exactly that many bots on each team (`--team-bots ALLIES,ENEMIES` with `--team-bot-skill SKILL,SKILL`, or `SHOOTER_TEAM_BOTS` and `SHOOTER_TEAM_BOT_SKILL` for the Docker image). Players take no bot's place, and they even out among themselves, the first on the allies. In free-for-all modes the two numbers add up to one bot count, at the first difficulty. Without `--team-bots`, the server fills both teams evenly from `--bots` ([modes.md](modes.md)).

## Private matches

Friends play a match of their own, as in Black Ops 2's private matches. The party leader sets it up, the whole party gets a server, and everyone picks a team and readies up in its lobby.

1. **Set up.** The party leader opens a mode's match setup on the title screen (signed in, with a party from Quick Play). They pick the map, each team's size and bot difficulty, and the game settings. Start reads **START PRIVATE MATCH**.
2. **The server.** `Matchmaking.start_private` calls `private_match_start` on the account server (`docker/online/nakama/modules/matchmaking.lua`). It creates a match for the party (`shooter_match.lua`) and tells each member to join it. The match handler asks the allocator for a private server: unranked, with the host's custom settings and bots per team, and no auto-start, so it waits in its lobby. Every player gets its address (marked `private`) and joins. A leader who spectates joins as a spectator.
3. **The lobby.** Beside the class menu, a panel lists both teams: who is ready, and the bots. Its **Switch team** button sends `ClientMessage::SwitchTeam`; the server moves the player. With bots per team set, each team keeps its own bot count; otherwise a bot of the team they join makes way and the bots refill the team they left. Its **Ready** button readies up. The match counts down once every player is ready, as any lobby does. Team swaps work in any team mode's lobby (not once the match is under way), in a local match too.

Without a party, or signed out, Start runs a local match.

![The lobby: both teams beside the class menu, with Switch team and Ready](media/private-lobby.jpg)

Black Ops 2 gives no XP in private matches. Here the game still reports a private match's XP itself, capped as for local matches; whether private matches should pay XP is an owner decision.

## Spectating

A player can join a match as a spectator, as Black Ops 2's CODcaster lets casters do. A spectator is on no team, has no body, and takes no place: bots fill the teams as if nobody had joined. Up to eight can watch one server. Ranked servers turn spectators away ("Spectating is off on ranked servers."), since a second client could watch for its team.

How to spectate:

- **Local match:** the **Spectate** toggle beside Game settings, to watch the bots.
- **Any server:** `--connect HOST[:PORT] --spectate`, or `"spectate": true` in `Net.join`'s options.

The controls (`godot/scripts/net/spectator.gd`):

| Action | Keyboard and mouse | Controller |
| --- | --- | --- |
| Next player, by team | Fire (left mouse) | Fire (right trigger) |
| Previous player | Aim (right mouse) | Aim (left trigger) |
| Switch between watching a player and the free camera | Interact (F) | Interact (X, or Square) |
| Fly the free camera | Move, Jump up, Crouch down, Sprint faster | the same |

Watching a player is first person, from their eyes, with their weapon. Their body hides, as in the killcam, and the killcam's machinery draws the view (`ViewDriver`, `killcam::eye`). The free camera starts where the view was.

![Watching a bot from its eyes, with both teams down the sides and its class below](media/spectate-follow.jpg)

![The free camera over Slamcity](media/spectate-free.jpg)

### The caster's view

The caster's view (`godot/scripts/net/caster_overlay.gd`) shows:

- **Each team down its side of the screen:** every player's name, kills and deaths, the weapon in hand, and the scorestreaks they hold. The dead are dimmed, and the watched player is highlighted.
- **A card for the watched player:** both weapons with their attachments, equipment, perks, and the count toward their next scorestreak.

The match HUD keeps the score, clock, and kill feed. The mode views show the objective (flags, zones, the charge), seen from the first team's side. The first team takes the ally colour and the second the enemy's. The minimap and the class menu are off.

### What the server sends spectators

- **Every player's snapshot**, with no own state and no minimap dots.
- **`GameEvent::Caster` twice a second while it changes:** every player's class (with the custom loadout rule applied), their scorestreaks, their count toward the next one, and what they hold. Players never get it, since it would tell them their enemies' classes.

Spectators only acknowledge snapshots; they send no inputs.

## The text form

Servers take the settings as `key=value` pairs split by commas (`--custom`, or `SHOOTER_CUSTOM` for the Docker image):

```sh
godot --headless -- --server --mode tdm --custom "hardcore=on,ff=reflect,killcam=off"
```

| Key | Values |
| --- | --- |
| `score` | a number |
| `time` | minutes, 0 for no clock |
| `rounds` | 1 to 20 |
| `respawn` | seconds, 0 to 30 |
| `health` | percent, 1 to 250 |
| `regen` | `normal`, `slow`, `fast`, `off` |
| `ff` | `off`, `on`, `reflect`, `shared` |
| `killcam`, `streaks`, `hardcore` | `on`, `off` |
| `radar` | `normal`, `constant`, `off` |
| `loadouts` | `any`, `default`, `pistols`, `snipers` |

`hardcore=on` sets hardcore's health and friendly fire, so put it first; later keys change them. An unknown key or value makes the server ignore the whole string (it logs why) and run the default rules. `Settings::to_text` writes only what differs from the defaults.

## On the wire

The welcome carries the settings after one flag bit: the default rules cost that bit alone. A client reads them with `NetClient.custom_settings()`; the server's are `NetServer.custom_rules()`.

## Tests

- `rust/sim/src/custom.rs`: the text form and the wire round trip, hardcore, regeneration, and limits.
- `godot/tests/custom_games_test.gd`: a client joins a hardcore server and predicts with 30 health; a server applies 30 health, instant respawns, a constant radar, and no scorestreaks; a ranked server ignores the settings.
