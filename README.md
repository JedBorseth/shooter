<img src="assets/icon.svg" width="64" height="64" alt="">

# Shooter

A minimal 2D shooter foundation built with Godot 4.7 (GDScript).

## Run

1. Install the standard Godot 4.7+ editor (no .NET required).
2. Import `project.godot`, then press **F6** to run the open scene or **F5** to run the project.
3. Move with **WASD** or the **arrow keys** and aim with the **mouse**.

From a terminal with Godot on your PATH:

```sh
godot --editor --path .
godot --path .
```

The starter arena includes a placeholder player, bounded movement, mouse aiming,
and a controls overlay. Weapons, enemies, scoring, and audio are intentionally
left for future gameplay work.

## Layout

- `scenes/main.tscn` — entry scene, arena, and HUD
- `scenes/player.tscn` — reusable player scene and collision shape
- `scripts/main.gd` — arena drawing and future game orchestration
- `scripts/player.gd` — movement and aiming
- `assets/` — shared artwork

Godot's `.godot/` cache and `builds/` exports stay out of Git. Commit generated
`.uid` files so resource references stay stable between collaborators.

## Collaborate

Clone the repository, open `project.godot`, and use the same Godot version.
Push a feature branch and open a pull request for changes:

```sh
git switch -c feature/my-change
git add .
git commit -m "Describe the change"
git push -u origin feature/my-change
```

Created in [T3 Code](https://t3.codes).
