extends Node
## Eye adaptation: the picture opens up in an alley, a doorway, or a tunnel, and
## darkens a little when the player looks into the sun, the way an eye or a
## camera adapts.
##
## The renderer's own auto-exposure averages the brightness of every frame on
## the GPU, which cost 0.24 ms on Medium on the reference card. This estimates
## the same thing from the world instead, a few times a second, for no GPU time:
## rays from the camera up into the sky tell how much of it is open, one toward
## the sun tells whether the player stands in sunlight and whether the sun in
## view is hidden. The estimate never sees muzzle flashes or explosions, so they
## can never pump the picture, and it eases slowly toward its target.
##
## Each map sets how far it may adapt (`exposure` in scripts/looks.gd), and
## `GraphicsQuality.set_adaptation` scales the exposure the map's look was
## graded at. `Settings` creates one for the whole game (none on a dedicated
## server).

const GraphicsQuality := preload("res://scripts/graphics.gd")
const Looks := preload("res://scripts/looks.gd")

## Probes per second.
const PROBE_RATE := 8.0
## How fast the exposure follows its target, per second, opening up and
## closing down (an eye closes down faster than it opens up).
const OPEN_RATE := 0.7
const CLOSE_RATE := 1.4
## Rays into the sky: straight up, then rings around it, turned with the view
## so that the ones in front count as much as the ones behind.
const SKY_RAYS: Array[Vector2] = [
	Vector2(0.0, 1.5708),
	Vector2(0.0, 0.9),
	Vector2(1.2566, 0.9),
	Vector2(2.5133, 0.9),
	Vector2(3.7699, 0.9),
	Vector2(5.0265, 0.9),
	Vector2(0.6283, 0.45),
	Vector2(-0.6283, 0.45),
]
## The share of a difference in light the eye makes up.
const ADAPT := 0.6
## How far a ray must reach to count as open sky, in metres.
const SKY_REACH := 60.0
const SUN_REACH := 2000.0
## Physics layer of the level geometry (project.godot: "world").
const WORLD_LAYER := 1

var _range := {}
var _scene := ""
var _stops := 0.0
var _target := 0.0
var _wait := 0.0
var _sun: DirectionalLight3D
## The light level the last probe found (light_level).
var _light := 1.0


func _init() -> void:
	name = "EyeAdaptation"
	process_mode = Node.PROCESS_MODE_ALWAYS


func _process(delta: float) -> void:
	var viewport := get_viewport()
	var camera := viewport.get_camera_3d() if viewport != null else null
	var path := GraphicsQuality.map_scene()
	if path != _scene:
		_scene = path
		_range = Looks.adaptation_for(path)
		_stops = 0.0
		_target = 0.0
		_wait = 0.0
	if camera == null or _range.is_empty():
		_apply(0.0)
		return
	_wait -= delta
	if _wait <= 0.0:
		_wait = 1.0 / PROBE_RATE
		_target = _probe(camera)
	var rate := CLOSE_RATE if _target < _stops else OPEN_RATE
	_stops = lerpf(_stops, _target, 1.0 - exp(-delta * rate))
	_apply(_stops)


## How much light the probe found, 0.165 (covered and in shade) to 1 (open
## sky and in sunlight): `open` is the share of sky rays that escape (0 to 1),
## `sunlit` whether the player stands in sunlight.
static func light_level(open: float, sunlit: float) -> float:
	return lerpf(0.3, 1.0, open) * lerpf(0.55, 1.0, sunlit)


## The exposure, in stops over the grade, for a light level against the level
## the map was graded at (`reference`): the eye makes up part of the difference
## (ADAPT), within `brighter` and `darker`, and closes down by up to `darker` as
## an unhidden sun comes into view (`into_sun`, 0 to 1).
static func target_stops(
	light: float, reference: float, into_sun: float, brighter: float, darker: float
) -> float:
	var stops := ADAPT * log(reference / maxf(light, 0.01)) / log(2.0)
	return clampf(stops - darker * into_sun, -darker, brighter)


func _probe(camera: Camera3D) -> float:
	var space := get_viewport().world_3d.direct_space_state
	var from := camera.global_position
	var yaw := camera.global_rotation.y
	var open := 0
	for ray: Vector2 in SKY_RAYS:
		var turn := ray.x + yaw
		var dir := Vector3(-sin(turn) * cos(ray.y), sin(ray.y), -cos(turn) * cos(ray.y))
		if _clear(space, from, from + dir * SKY_REACH):
			open += 1
	var sunlit := 0.0
	var into_sun := 0.0
	if _find_sun():
		var to_sun := _sun.global_basis.z.normalized()
		if to_sun.y > 0.0 and _clear(space, from, from + to_sun * SUN_REACH):
			sunlit = 1.0
			var forward := -camera.global_basis.z.normalized()
			into_sun = smoothstep(0.8, 0.97, forward.dot(to_sun))
	var light := light_level(float(open) / SKY_RAYS.size(), sunlit)
	_light = light
	return target_stops(light, _range["reference"], into_sun, _range["brighter"], _range["darker"])


func _clear(space: PhysicsDirectSpaceState3D, from: Vector3, to: Vector3) -> bool:
	var query := PhysicsRayQueryParameters3D.create(from, to, WORLD_LAYER)
	return space.intersect_ray(query).is_empty()


func _apply(stops: float) -> void:
	GraphicsQuality.set_adaptation(get_tree(), stops)


## The sun of the map in view: the first adopted directional light.
func _find_sun() -> bool:
	if is_instance_valid(_sun) and _sun.is_inside_tree() and _sun.visible:
		return true
	_sun = null
	for node: Node in get_tree().get_nodes_in_group(&"graphics_adopted"):
		if node is DirectionalLight3D and (node as DirectionalLight3D).visible:
			_sun = node
			return true
	return false
