<img src="godot/assets/icon.svg" width="64" height="64" alt="">

# Shooter

A multiplayer first-person shooter built on Godot 4.7, with gameplay code written in Rust. It runs on the Forward+ renderer with Jolt physics.

The plan, decisions, and reference projects are in [docs/roadmap.md](docs/roadmap.md). Open tasks for contributors are in [TODO.md](TODO.md).

![First-person views: hip fire, aimed down sights, firing, sprinting, reloading, and the pistol aimed](docs/media/phase1-weapons.jpg)

## Layout

- `godot/`: the Godot project (scenes, GDScript glue, assets).
- `rust/sim/`: engine-independent, deterministic simulation: movement, weapons (fire rate, falloff, recoil patterns, spread, reloads, ADS, switching), and the combined per-player step that netcode will predict. No Godot types, so the client and the dedicated server share it and it can be unit-tested on its own.
- `rust/game/`: the GDExtension that exposes the simulation to Godot: `FpsPlayer` (input, hitscan, camera), `ViewmodelRig` and `WeaponView` (procedural weapon motion, muzzle flash, sounds), and `TargetDummy`.
- `godot/shaders/`: viewmodel projection (own FOV, never clips walls), muzzle flash, and the blockout grid.
- `docs/`: roadmap and design notes.

## Set up a Linux playtest machine

1. Install a current GPU driver with Vulkan support (NVIDIA proprietary driver, or Mesa for AMD and Intel). `vulkaninfo --summary` should list your GPU.
2. Install Rust: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`.
3. Install Godot 4.7.2 (standard build, no .NET) from the [4.7.2 release page](https://github.com/godotengine/godot/releases/tag/4.7.2-stable). Unzip it and put the binary on your PATH as `godot`.

## Build and run

Requirements: Godot 4.7.2+ (standard build, no .NET) and a stable Rust toolchain.

```sh
cd rust && cargo build          # builds rust/target/debug/libshooter_game.*
cd .. && godot --path godot     # run the game
godot --editor --path godot     # open the editor
```

Rebuild with `cargo build` after changing Rust code. The editor hot-reloads the extension.

## Test

```sh
cd rust && cargo test && cargo clippy --all-targets
cd .. && godot --headless --path godot --script res://tests/smoke_test.gd
```

The smoke test loads the arena headless and drives the player through movement (toggle sprint, slide, crouch) and combat (aim, fire, kill a dummy, reload, switch). It exits non-zero on failure.

To review visuals, capture a set of first-person screenshots (hip, ADS, firing, sprint, reload, pistol, impacts). This needs a GPU:

```sh
CAPTURE_DIR=/tmp/shots godot --path godot --resolution 1600x900 --script res://tests/capture_screens.gd
```

## Controls

| Action | Input |
| --- | --- |
| Move / look | WASD / mouse |
| Sprint (toggle; ends when you stop moving forward, fire, or aim) | Shift |
| Crouch (toggle; slides while sprinting) | C or Ctrl |
| Jump | Space |
| Fire / aim down sights | Left / right mouse |
| Reload | R |
| Switch weapon | 1, 2, or mouse wheel |
| Release mouse | Esc |

The arena has target dummies at several ranges. They show their health and respawn 3 seconds after being killed.

## Assets

Placeholder art and audio are CC0. Sources are listed in [godot/assets/CREDITS.md](godot/assets/CREDITS.md).
