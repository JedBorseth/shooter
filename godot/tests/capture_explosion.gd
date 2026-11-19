extends SceneTree
## Captures an explosion (`CombatEffects.explosion`, a frag's size) as a frame
## strip in three lights: the Testing Area in the sun (10 m off, and 5 m off
## looking down at the chips and the scorch), inside its culvert in shade,
## and the Zombies arena at night. Needs a GPU and a display (or Xvfb),
## and --fixed-fps 60 so the strip's moments are true time:
##   CAPTURE_DIR=/path/to/out CAPTURE_PRESET=medium godot --path godot \
##     --resolution 1600x900 --audio-driver Dummy --fixed-fps 60 \
##     --script res://tests/capture_explosion.gd
##
## Environment:
##   CAPTURE_DIR     output directory (required).
##   CAPTURE_PRESET  low, medium, or high (default medium).
##   CAPTURE_ONLY    comma list of lights: sun, close, shade, night (default: all).

const Benchmark := preload("res://tests/benchmark.gd")
## Sixtieths of a second after the blast for each frame of the strip.
const MOMENTS := [2, 5, 9, 15, 24, 40, 70, 120, 200, 320, 600]
## Per light: the map, the player's feet, yaw, pitch, and where the blast goes
## off (on the ground about 10 m ahead).
const LIGHTS := {
	"sun": ["res://scenes/main.tscn", Vector3(0, 0.1, 20), 0.0, -0.06, Vector3(0, 0.05, 10)],
	"close": ["res://scenes/main.tscn", Vector3(0, 0.1, 20), 0.0, -0.35, Vector3(0.5, 0.05, 15)],
	"shade": ["res://scenes/main.tscn", Vector3(-10, 0.1, -9), 0.0, -0.08, Vector3(-10, 0.05, -17)],
	"night":
	["res://scenes/zombies_arena.tscn", Vector3(0, 0.1, 8), 0.0, -0.06, Vector3(0, 0.05, -2)],
}

var _out := OS.get_environment("CAPTURE_DIR")
var _only := OS.get_environment("CAPTURE_ONLY")
var _player: FpsPlayer
var _settings: Node


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var preset := OS.get_environment("CAPTURE_PRESET")
	if not Benchmark.PRESET_KEYS.has(preset):
		preset = "medium"
	_settings = root.get_node("Settings")
	_settings.path = "user://capture_explosion_settings.cfg"
	_settings.set_graphics_preset(Benchmark.PRESET_KEYS[preset])
	for light: String in LIGHTS:
		if not _only.is_empty() and not light in _only.split(","):
			continue
		var spec: Array = LIGHTS[light]
		await _load(spec[0])
		await _hold(spec[1], spec[2], spec[3], 60)
		var effects := get_first_node_in_group("combat_effects")
		if effects == null:
			push_error("No combat effects in %s." % spec[0])
		else:
			effects.explosion(spec[4], Vector3.UP, 1.0)
			var shown := 0
			for moment: int in MOMENTS:
				await _hold(spec[1], spec[2], spec[3], moment - shown)
				shown = moment
				await _shot("%s_%s_%03d" % [preset, light, moment])
		_unload()
		await process_frame
	_settings.reset_to_defaults()
	DirAccess.remove_absolute(ProjectSettings.globalize_path(_settings.path))
	quit()


func _load(path: String) -> void:
	var scene: Node = (load(path) as PackedScene).instantiate()
	var sentry := scene.get_node_or_null("Sentry")
	if sentry != null:
		sentry.set("enabled", false)
	var director := scene.get_node_or_null("Director")
	if director != null:
		director.process_mode = Node.PROCESS_MODE_DISABLED
	root.add_child(scene)
	current_scene = scene
	_player = scene.get_node("Player") as FpsPlayer


func _unload() -> void:
	current_scene.queue_free()
	current_scene = null
	_player = null


func _hold(at: Vector3, yaw: float, pitch: float, frames: int) -> void:
	for i in frames:
		_player.global_position = at
		_player.velocity = Vector3.ZERO
		_player.set_look(yaw, pitch)
		await process_frame


func _shot(name: String) -> void:
	await RenderingServer.frame_post_draw
	root.get_viewport().get_texture().get_image().save_png(_out.path_join(name + ".png"))
