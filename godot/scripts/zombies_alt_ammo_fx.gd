extends Node3D
## A re-packed weapon's alternate ammo going off (`ZombiesEvent::AltAmmoFired`,
## `net/horde_view.rs`; rules in `rust/sim/src/zombies/alt_ammo.rs`):
##
## - "arc": a jagged bolt jumps from the zombie hit to each one it strikes in
##   turn, 0.11 s a jump, with a cold flash at each;
## - "cinder": a burst of flame where the zombie was hit, burning on for the
##   three seconds its neighbours take to die (the fire effect,
##   `scenes/effects/fire.tscn`).
##
## Effects, so the bolt is a ribbon mesh built here, not a model.

const ARC_COLOUR := Color(0.62, 0.86, 1.0)
const CINDER_COLOUR := Color(1.0, 0.55, 0.2)
## Seconds per jump of the Arc, and how long each bolt stays lit.
const JUMP := 0.11
const BOLT_LIFE := 0.25
## The bolt's width (metres).
const BOLT_WIDTH := 0.05
const FIRE := preload("res://scenes/effects/fire.tscn")
const CINDER_SECONDS := 3.0

## Captures hold the effect at this many seconds in (-1 runs it).
var held_at := -1.0

var _kind := ""
var _points := PackedVector3Array()
var _age := 0.0
var _mesh: ImmediateMesh
var _material: StandardMaterial3D
var _light: OmniLight3D


## Plays `kind` ("arc" or "cinder") through `points`: where it went off,
## then (the Arc) each zombie struck in turn.
func setup(kind: String, points: PackedVector3Array) -> void:
	_kind = kind
	_points = points
	if points.is_empty():
		queue_free()
		return
	global_position = Vector3.ZERO
	_light = OmniLight3D.new()
	_light.shadow_enabled = false
	_light.omni_range = 5.0
	_light.light_color = ARC_COLOUR if kind == "arc" else CINDER_COLOUR
	_light.light_energy = 4.0
	_light.position = points[0]
	add_child(_light)
	if kind == "cinder":
		var fire := FIRE.instantiate() as Node3D
		add_child(fire)
		fire.global_position = points[0] - Vector3.UP
		fire.call("start", 1.6, CINDER_SECONDS)
		return
	_mesh = ImmediateMesh.new()
	var bolts := MeshInstance3D.new()
	bolts.mesh = _mesh
	bolts.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_material = StandardMaterial3D.new()
	_material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	_material.cull_mode = BaseMaterial3D.CULL_DISABLED
	_material.albedo_color = Color(ARC_COLOUR.r * 2.5, ARC_COLOUR.g * 2.5, ARC_COLOUR.b * 2.5)
	bolts.material_override = _material
	add_child(bolts)


func _process(delta: float) -> void:
	_age = held_at if held_at >= 0.0 else _age + delta
	if _kind == "cinder":
		_light.light_energy = 4.0 * maxf(0.0, 1.0 - _age / 0.6)
		if _age > CINDER_SECONDS + 2.5:
			queue_free()
		return
	var last := (_points.size() - 1) * JUMP + BOLT_LIFE
	if _age > last:
		queue_free()
		return
	_mesh.clear_surfaces()
	var live: Array[int] = []
	for i in range(1, _points.size()):
		var start := (i - 1) * JUMP
		if _age >= start and _age <= start + BOLT_LIFE:
			live.append(i)
	_light.light_energy = 4.0 if not live.is_empty() else 0.0
	if live.is_empty():
		return
	_mesh.surface_begin(Mesh.PRIMITIVE_TRIANGLES)
	for i in live:
		_bolt(_points[i - 1], _points[i])
		_light.position = _points[i]
	_mesh.surface_end()


## A jagged bolt from `a` to `b`, re-drawn each frame so it crackles, with
## a short fork off its middle: ribbons turned to face the camera.
func _bolt(a: Vector3, b: Vector3) -> void:
	var steps := 8
	var previous := a
	var span := a.distance_to(b)
	var jitter := span * 0.08
	for s in range(1, steps + 1):
		var point := a.lerp(b, float(s) / steps)
		if s < steps:
			point += Vector3(
				randf_range(-jitter, jitter),
				randf_range(-jitter, jitter),
				randf_range(-jitter, jitter)
			)
		_ribbon(previous, point, BOLT_WIDTH)
		previous = point
	var mid := a.lerp(b, 0.5)
	_ribbon(
		mid,
		mid + Vector3(randf_range(-1, 1), 0.5, randf_range(-1, 1)) * span * 0.2,
		BOLT_WIDTH * 0.5
	)


## One segment of ribbon from `a` to `b`, `width` wide, facing the camera.
func _ribbon(a: Vector3, b: Vector3, width: float) -> void:
	var camera := get_viewport().get_camera_3d()
	var eye := camera.global_position if camera != null else a + Vector3.BACK
	var side := (b - a).cross(eye - a).normalized() * width * 0.5
	for v in [a - side, a + side, b + side, a - side, b + side, b - side]:
		_mesh.surface_add_vertex(v)
