extends SceneTree
## Captures frame strips of soldiers dying, for review: shot in the chest from
## the front, back, and side, a headshot, a shotgun blast, a crouched death, a
## death mid-air, and a death mid-sprint. Each strip is eight moments from the
## shot to the body at rest, seen from the side, as one image per death. The
## `flinch_*` strips are hits that do not kill: head, chest, arm, and leg.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1280x720 \
##     --fixed-fps 120 --script res://tests/capture_deaths.gd
## CAPTURE_SETS picks deaths (comma list of the names below; default all).

const SOLDIER := preload("res://scenes/characters/soldier.tscn")
## Seconds after the shot each strip shows: the last is past the 5 s cap,
## where every body has stopped.
const MOMENTS := [0.0, 0.08, 0.17, 0.3, 0.5, 1.0, 2.5, 5.5]
const SPOT := Vector3(0, 0.05, 20)
## name: [travel (world), hit shape, force, start state]
const DEATHS := {
	"chest_front": [Vector3(0, 0, 1), 2, 3.0, {}],
	# A knife kill from the front (`HitPart::KNIFE_FORCE`): it crumples.
	"knife": [Vector3(0, 0, 1), 2, 0.0, {}],
	"chest_back": [Vector3(0, 0, -1), 2, 3.0, {}],
	"chest_left": [Vector3(1, 0, 0), 2, 3.0, {}],
	"headshot": [Vector3(0, 0, 1), 0, 3.0, {}],
	"shotgun": [Vector3(0, 0, 1), 2, 15.0, {}],
	"sniper_leg": [Vector3(1, 0, 0), 10, 11.0, {}],
	"crouched": [Vector3(0, 0, 1), 2, 3.0, {"stance": "crouching", "crouch": 1.0}],
	"mid_air": [Vector3(0, 0, 1), 2, 3.0, {"on_floor": false, "height": 1.0}],
	"sprinting": [Vector3(1, 0, 0), 2, 3.0, {"sprinting": true, "velocity": Vector3(0, 0, -6.5)}],
	# Hits that do not kill: the reactions, closer and over a shorter time.
	"flinch_head": [Vector3(1, 0, 0), 0, 3.0, {}, true],
	"flinch_chest": [Vector3(0, 0, 1), 2, 3.0, {}, true],
	"flinch_arm": [Vector3(1, 0, 0), 7, 3.0, {}, true],
	"flinch_leg": [Vector3(0, 0, 1), 10, 3.0, {}, true],
}
## Seconds after a hit each flinch strip shows.
const FLINCH_MOMENTS := [0.0, 0.03, 0.06, 0.1, 0.15, 0.22, 0.32, 0.5]

var _out := OS.get_environment("CAPTURE_DIR")
var _sets := OS.get_environment("CAPTURE_SETS")
var _camera: Camera3D


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var scene: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(scene)
	current_scene = scene
	for name in ["Sentry"]:
		var node := scene.get_node_or_null(name)
		if node != null:
			node.process_mode = Node.PROCESS_MODE_DISABLED
	var player := scene.get_node_or_null("Player") as FpsPlayer
	if player != null:
		player.set_input_enabled(false)
		player.process_mode = Node.PROCESS_MODE_DISABLED
		player.visible = false
	for layer: CanvasLayer in root.find_children("*", "CanvasLayer", true, false):
		layer.visible = false
	_camera = Camera3D.new()
	_camera.fov = 50.0
	scene.add_child(_camera)
	_camera.make_current()
	await _frames(30)
	for death: String in DEATHS:
		if _sets.is_empty() or death in _sets.split(","):
			await _strip(scene, death, DEATHS[death])
	quit()


func _strip(scene: Node, death: String, spec: Array) -> void:
	var travel: Vector3 = spec[0]
	var state: Dictionary = spec[3].duplicate()
	var flinch: bool = spec.size() > 4 and spec[4]
	var moments: Array = FLINCH_MOMENTS if flinch else MOMENTS
	var rig := SOLDIER.instantiate() as SoldierRig
	scene.add_child(rig)
	var velocity: Vector3 = state.get("velocity", Vector3.ZERO)
	# Moving bodies start back along their path, so they die in frame.
	var at := SPOT + Vector3.UP * float(state.get("height", 0.0)) - velocity * 40.0 / 60.0
	rig.global_position = at
	state["weapon"] = "m4"
	state["velocity"] = velocity
	# Seen from the soldier's right, a little in front, far enough for the fall.
	if flinch:
		_camera.global_position = SPOT + Vector3(2.0, 1.5, -1.2)
		_camera.look_at(SPOT + Vector3(0, 1.1, -0.1))
	else:
		_camera.global_position = SPOT + Vector3(4.2, 1.4, -0.6)
		_camera.look_at(SPOT + Vector3(0, 0.6, -0.4))
	for i in 80:
		rig.drive(state, 1.0 / 120.0)
		rig.global_position += velocity / 120.0
		await process_frame
	var point := rig.global_position + Vector3.UP * (1.62 if spec[1] == 0 else 1.25)
	rig.take_hit_at(travel, not flinch, point, spec[1], spec[2])
	var dead := state.duplicate()
	dead["alive"] = flinch
	var frames: Array[Image] = []
	var t := 0.0
	var next := 0
	while next < moments.size():
		if t + 0.0001 >= moments[next]:
			await RenderingServer.frame_post_draw
			frames.append(root.get_texture().get_image())
			next += 1
		rig.drive(dead, 1.0 / 120.0)
		await process_frame
		t += 1.0 / 120.0
	_save(death, frames)
	rig.queue_free()
	for gun in scene.find_children("DroppedWeapon", "", true, false):
		gun.queue_free()
	await _frames(5)


func _save(death: String, frames: Array[Image]) -> void:
	var w := 480
	var h := 270
	var strip := Image.create(w * 4, h * 2, false, Image.FORMAT_RGB8)
	for i in frames.size():
		var f := frames[i]
		f.convert(Image.FORMAT_RGB8)
		f.resize(w, h, Image.INTERPOLATE_BILINEAR)
		strip.blit_rect(f, Rect2i(0, 0, w, h), Vector2i((i % 4) * w, (i / 4) * h))
	strip.save_png(_out.path_join("death_%s.png" % death))
	print("saved death_%s.png" % death)


func _frames(count: int) -> void:
	for i in count:
		await process_frame
