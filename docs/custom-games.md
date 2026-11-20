# Custom games

A host can change a match's rules before it starts, as in Black Ops 2's Custom Games. The rules live in `rust/sim/src/custom.rs` (`custom::Settings`). The server takes them on the command line, applies them, and sends them to every player in the welcome. Prediction (health and its regeneration) and the HUD follow the same rules.

Hardcore's missing HUD lands with the setup screen (TODO.md, Custom games).

Ranked servers (`--accounts`) ignore custom settings: ranked play always uses the default rules. Zombies keeps its own rules.

## Settings

Each setting follows a Black Ops 2 game type setting (the `getgametypesetting` key in its `_globallogic.gsc` is in brackets).

| Setting | Choices (default first) | What it does |
| --- | --- | --- |
| Score limit (`scoreLimit`) | the mode's, or any number | Kills, captures, or points to win. Search and Destroy uses rounds instead. |
| Time limit (`timeLimit`) | the server's, or minutes; 0 is no clock | Search and Destroy has no match clock. |
| Rounds (`roundWinLimit`) | the server's, or 1 to 20 | Search and Destroy's rounds to win. |
| Respawn delay (`playerRespawnDelay`) | the mode's (3 s, 5 s in Capture the Flag), or 0 to 30 s | How long a dead player waits. A killcam still holds the respawn until it ends or is skipped. |
| Health (`playerMaxHealth`) | 100%, 30%, 50%, 150%, 200% | Health a life starts with. Weapon damage does not change, so 30% means almost every hit kills. |
| Health regeneration (`playerHealthRegenTime`) | Normal, Slow, Fast, Off | Slow waits twice as long and refills at half the rate; Fast is the opposite. The refill rate scales with health, so a full refill takes as long at any health. |
| Friendly fire (`friendlyfiretype`) | Off, On, Reflect, Shared | On: teammates take full damage. Reflect: the attacker takes half instead. Shared: each takes half. Half is Black Ops 2's: `int(damage * 0.5)`, at least 1. |
| Killcam (`allowKillcam`) | On, Off | Off: no killcam on death and no final or round-winning killcam. |
| Radar (`forceRadar`) | Normal, Constant, Off | Constant: a UAV for every side that never goes down, sweeping every enemy onto the minimap every 3 s (Ghost still hides a moving player). Off: no minimap, and the server sends no dots. |
| Scorestreaks (`custom_killstreak_mode`) | On, Off | Off: points earn no scorestreaks. |
| Loadouts (`disableCAC` and party restrictions) | Any, Default classes, Pistols only, Snipers only | Which classes players may use. |
| Hardcore (`hardcoreMode`) | Off, On | No HUD. Turning it on sets health to 30% and friendly fire to On, Black Ops 2's hardcore rules; either can still be changed. |

## Friendly fire

With friendly fire on, bullets, the knife, grenades, and every other blast (launchers, scorestreaks, props) hurt teammates too; bullets no longer pass through them. What a hit on a teammate does depends on the setting (`FriendlyFire::split`):

- **On:** the teammate takes it all.
- **Reflect:** the teammate takes nothing, and the attacker takes half (at least 1).
- **Shared:** each takes half (at least 1).

A teamkill shows in the kill feed and counts as the victim's death, but scores nothing: no kill points, no team score, no streak count, and no objective bonus. Death by reflected damage is a suicide. Black Ops 2 also punishes repeat teamkillers (`teamKillPunishCount`: a respawn delay, then a kick); that is not built yet.

## Loadout rules

The server applies the rule to the class each life spawns with (`Loadouts::restrict`), players' and bots' alike; equipment and perks stay.

- **Default classes:** a custom class becomes the first default class.
- **Pistols only:** no primary; the class's pistol, or the M1911.
- **Snipers only:** the class's sniper rifle, or the M40; the secondary only if it is a pistol.

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
