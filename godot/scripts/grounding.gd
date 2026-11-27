extends MeshInstance3D
## Contact shadows for characters: a soft dark patch on the ground under every
## soldier, so they stand on the ground instead of floating above it
## where the sun's shadow is soft, far away, or falls behind them, and in the
## shade, where there is no sun shadow at all.
##
## `Settings` calls `attach` for every `SoldierRig` that enters the tree on a
## client (the dedicated server draws nothing). Each patch is a flat quad drawn
## with shaders/contact_shadow.gdshader, so it costs only the pixels it covers.
## (A Decal looks the same but turns on decal shading for every pixel of a map
## that has no other decals, 0.14 ms on Medium in the Testing Area; a patch
## that reads the depth buffer makes the renderer copy it, 0.1 ms.) A short ray
## down from the feet, a few times a second, lays the patch on the ground and
## tilts it to a slope, and fades it as the feet leave the ground.
##
## The patch follows the body: a small oval round the feet when standing,
## longer and darker as the body gets low (a crouch pushes the knees forward, a
## slide lays the body out behind the feet), turned to face where the body
## faces. In direct sun the sun's own shadow already grounds the feet, so the
## patch fades to a faint core there (a second ray, toward the sun, tells sun
## from shade); out past the sun's shadow distance it is full again.

const SHADER := preload("res://shaders/contact_shadow.gdshader")
## Edge of the shared quad, in metres; each patch scales it to its stance.
const SIZE := 1.0
## Darkness at the centre standing: the ground keeps 1 - STRENGTH of its light.
const STRENGTH := 0.6
## The patch's footprint in each stance, in metres: across the body, along it
## (the way it faces), and how far its centre sits ahead of the feet. Darkness
## is the strength at the centre. A crouch kneels over the feet; a slide lays
## the body out behind them.
const STANDING := {"width": 0.85, "length": 0.75, "ahead": 0.0, "darkness": STRENGTH}
const CROUCHING := {"width": 0.95, "length": 1.1, "ahead": 0.1, "darkness": 0.68}
const SLIDING := {"width": 0.85, "length": 1.8, "ahead": -0.5, "darkness": 0.75}
## In direct sun the patch keeps this share of its darkness: just a core under
## the feet, where the sun's soft shadow map loses the contact.
const SUN_KEEP := 0.4
## The sun test: a ray from this height above the ground toward the sun.
const SUN_RAY_FROM := 0.5
const SUN_RAY_LENGTH := 60.0
## How quickly the patch follows a change between sun and shade (per test).
const SUN_FOLLOW := 0.35
## Distance from the camera where the patch starts to fade, and how long it takes.
const FADE_BEGIN := 60.0
const FADE_LENGTH := 20.0
## The patch sits this far above the ground, so it never sinks into it.
const LIFT := 0.02
## The ray starts this far above the feet and looks this far below them.
const RAY_UP := 0.3
const RAY_DOWN := 1.5
## Height of the feet above the ground where the patch has faded out.
const GONE_AT := 1.0
## Physics layer of the level geometry (project.godot: "world").
const WORLD_LAYER := 1
## The ground is tested every this many physics ticks (120 a second), and
## the sun every SUN_EVERY ground tests.
const TEST_EVERY := 6
const SUN_EVERY := 2

static var _mesh: PlaneMesh
static var _material: ShaderMaterial
static var _count := 0
## The sun's shadow distance last given to the material.
static var _shadow_end := -1.0

var _rig: Node3D
var _tick := 0
var _tests := 0
## 1 in direct sun, 0 in shade, eased between the two.
var _sunlit := 0.0
var _sun_known := false
## The patch centre's offset from the feet along the ground.
var _ahead := Vector3.ZERO


## Adds a patch under `rig` (a SoldierRig). Safe to call more than once.
static func attach(rig: Node3D) -> void:
	if not is_instance_valid(rig) or rig.has_node(^"ContactShadow"):
		return
	var patch: MeshInstance3D = preload("res://scripts/grounding.gd").new()
	patch.name = "ContactShadow"
	rig.add_child(patch)


static func shared_mesh() -> PlaneMesh:
	if _mesh == null:
		_mesh = PlaneMesh.new()
		_mesh.size = Vector2(SIZE, SIZE)
	return _mesh


static func shared_material() -> ShaderMaterial:
	if _material == null:
		_material = ShaderMaterial.new()
		_material.shader = SHADER
		_material.set_shader_parameter(&"fade_begin", FADE_BEGIN)
		_material.set_shader_parameter(&"fade_length", FADE_LENGTH)
		_material.set_shader_parameter(&"sun_keep", SUN_KEEP)
	return _material


## The footprint for a crouch blend and a slide blend (each 0 to 1).
static func footprint(crouch: float, slide: float) -> Dictionary:
	var out := {}
	for key: String in STANDING:
		var value: float = lerpf(STANDING[key], CROUCHING[key], crouch)
		out[key] = lerpf(value, SLIDING[key], slide)
	return out


## The map's sun casting shadows, if it has one.
static func sun_of(tree: SceneTree) -> DirectionalLight3D:
	for node: Node in tree.get_nodes_in_group(&"graphics_adopted"):
		var light := node as DirectionalLight3D
		if light != null and light.visible and light.shadow_enabled and light.is_inside_tree():
			return light
	return null


func _init() -> void:
	mesh = shared_mesh()
	material_override = shared_material()
	cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	gi_mode = GeometryInstance3D.GI_MODE_DISABLED
	# Past the fade, skip it altogether.
	visibility_range_end = FADE_BEGIN + FADE_LENGTH
	# Placed in world space from the ground test, not by the rig.
	top_level = true
	# Characters spawned together test the ground on different ticks.
	_tick = _count % TEST_EVERY
	_count += 1


func _ready() -> void:
	_rig = get_parent() as Node3D
	_place()


func _physics_process(_delta: float) -> void:
	_tick += 1
	if _tick % TEST_EVERY == 0:
		_place()
	elif _rig != null and visible:
		# Between tests, follow the rig across the ground.
		var at := global_position
		at.x = _rig.global_position.x + _ahead.x
		at.z = _rig.global_position.z + _ahead.z
		global_position = at


## Lays the patch on the ground under the rig, shaped and turned by its
## stance, or hides it with no ground near.
func _place() -> void:
	if _rig == null or not is_inside_tree():
		return
	var feet := _rig.global_position
	var space := get_world_3d().direct_space_state
	var query := PhysicsRayQueryParameters3D.create(
		feet + Vector3.UP * RAY_UP, feet + Vector3.DOWN * RAY_DOWN, WORLD_LAYER
	)
	var hit := space.intersect_ray(query)
	if hit.is_empty():
		visible = false
		return
	visible = true
	var ground: Vector3 = hit["position"]
	var normal: Vector3 = hit["normal"]
	var up := normal if normal.y > 0.5 else Vector3.UP
	var blend := Vector2.ZERO
	if _rig.has_method(&"stance_blend"):
		blend = _rig.call(&"stance_blend")
	var shape := footprint(blend.x, blend.y)
	# Along the ground the way the body faces (the rig faces -Z).
	var facing := -_rig.global_basis.z
	facing = (facing - up * facing.dot(up)).normalized()
	if not facing.is_finite() or facing.length_squared() < 0.5:
		facing = Vector3.FORWARD
	var side := facing.cross(up).normalized()
	_ahead = facing * float(shape["ahead"])
	# The plane's X runs across the body and its Z along it (+Z behind).
	global_transform = Transform3D(
		Basis(side * float(shape["width"]), up, -facing * float(shape["length"])),
		ground + up * LIFT + _ahead
	)
	var height := maxf(feet.y - ground.y, 0.0)
	set_instance_shader_parameter(&"shown", 1.0 - smoothstep(0.0, GONE_AT, height))
	set_instance_shader_parameter(&"darkness", float(shape["darkness"]))
	_tests += 1
	if _tests % SUN_EVERY == 0 or not _sun_known:
		_test_sun(space, ground + up * SUN_RAY_FROM)


## Whether the ground under the rig is in direct sun, eased so a step from
## shade into sun fades instead of popping.
func _test_sun(space: PhysicsDirectSpaceState3D, from: Vector3) -> void:
	var sun := sun_of(get_tree())
	var lit := 0.0
	if sun != null:
		var to_sun := sun.global_basis.z.normalized()
		# Below the horizon the sun lights nothing.
		if to_sun.y > 0.02:
			var ray := PhysicsRayQueryParameters3D.create(
				from, from + to_sun * SUN_RAY_LENGTH, WORLD_LAYER
			)
			lit = 1.0 if space.intersect_ray(ray).is_empty() else 0.0
		if not is_equal_approx(sun.directional_shadow_max_distance, _shadow_end):
			_shadow_end = sun.directional_shadow_max_distance
			shared_material().set_shader_parameter(&"sun_shadow_end", _shadow_end)
	_sunlit = lit if not _sun_known else lerpf(_sunlit, lit, SUN_FOLLOW)
	_sun_known = true
	set_instance_shader_parameter(&"sunlit", _sunlit)
