extends SceneTree
## Every playable map pushes Settings onto its player: with Input device set to
## Controller, the right stick turns the view on each map. Nuketown and
## Warehouse once left their pause menu without a player, so the player never
## learned a controller was in use and stick look did nothing there.
## godot --headless --path godot --script res://tests/map_settings_test.gd

const TEST_PATH := "user://map_settings_test.cfg"
const MAPS: Array[String] = [
	"res://scenes/main.tscn",
	"res://scenes/slamcity.tscn",
	"res://scenes/nuketown.tscn",
	"res://scenes/warehouse.tscn",
	"res://scenes/breakwater.tscn",
	"res://scenes/zombies_arena.tscn",
]

var _failures := 0


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var settings: Node = root.get_node("Settings")
	settings.path = TEST_PATH
	settings.reset_to_defaults()
	settings.set_setting(&"input_device", settings.InputDevice.CONTROLLER)
	for path in MAPS:
		await _check_map(settings, path)
	settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(TEST_PATH))
	_finish()


func _check_map(settings: Node, path: String) -> void:
	var map := (load(path) as PackedScene).instantiate()
	root.add_child(map)
	await process_frame
	var player := map.get_node_or_null("Player") as FpsPlayer
	if player == null:
		_check(false, "%s has a Player" % path)
	else:
		_check(player.controller_input, "%s: the player knows a controller plays" % path)
		# A right-stick push at full tilt turns the view within a few frames.
		var camera := player.get_node("Camera") as Node3D
		var before := camera.global_rotation.y
		Input.action_press(&"look_right", 1.0)
		for i in 10:
			await process_frame
		Input.action_release(&"look_right")
		var turned := absf(angle_difference(before, camera.global_rotation.y))
		_check(turned > 0.01, "%s: the right stick turns the view (%.3f rad)" % [path, turned])
		settings.set_setting(&"stick_sensitivity_level", 9)
		_check(
			is_equal_approx(
				player.stick_yaw_speed, settings.STICK_YAW_SPEED * settings.sensitivity_scale(9)
			),
			"%s: a look speed change reaches the player" % path
		)
		settings.set_setting(
			&"stick_sensitivity_level", settings.DEFAULTS[&"stick_sensitivity_level"]
		)
	map.queue_free()
	await process_frame


func _check(condition: bool, message: String) -> void:
	if condition:
		print("ok   ", message)
	else:
		_failures += 1
		printerr("FAIL ", message)


func _finish() -> void:
	if _failures > 0:
		printerr("%d check(s) failed" % _failures)
	else:
		print("All map settings checks passed.")
	quit(1 if _failures > 0 else 0)
