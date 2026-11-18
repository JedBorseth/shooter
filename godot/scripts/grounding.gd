extends MeshInstance3D
## Contact shadows for characters: a soft dark patch on the ground under every
## soldier and zombie, so they stand on the ground instead of floating above it
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

const SHADER := preload("res://shaders/contact_shadow.gdshader")
## Width of the patch on the ground, in metres: a little wider than a stance.
const SIZE := 1.0
## Darkness at the centre: the ground keeps 1 - STRENGTH of its light.
const STRENGTH := 0.6
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
## The ground is tested every this many physics ticks (120 a second).
const TEST_EVERY := 6

static var _mesh: PlaneMesh
static var _material: ShaderMaterial
static var _count := 0

var _rig: Node3D
var _tick := 0


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
		_material.set_shader_parameter(&"strength", STRENGTH)
		_material.set_shader_parameter(&"fade_begin", FADE_BEGIN)
		_material.set_shader_parameter(&"fade_length", FADE_LENGTH)
	return _material


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
	elif _rig != null:
		# Between tests, follow the rig across the ground.
		var at := global_position
		at.x = _rig.global_position.x
		at.z = _rig.global_position.z
		global_position = at


## Lays the patch on the ground under the rig, or hides it with no ground near.
func _place() -> void:
	if _rig == null or not is_inside_tree():
		return
	var feet := _rig.global_position
	var query := PhysicsRayQueryParameters3D.create(
		feet + Vector3.UP * RAY_UP, feet + Vector3.DOWN * RAY_DOWN, WORLD_LAYER
	)
	var hit := get_world_3d().direct_space_state.intersect_ray(query)
	if hit.is_empty():
		visible = false
		return
	visible = true
	var ground: Vector3 = hit["position"]
	var normal: Vector3 = hit["normal"]
	var up := normal if normal.y > 0.5 else Vector3.UP
	var side := up.cross(Vector3.FORWARD).normalized()
	global_transform = Transform3D(Basis(side, up, side.cross(up)), ground + up * LIFT)
	var height := maxf(feet.y - ground.y, 0.0)
	set_instance_shader_parameter(&"shown", 1.0 - smoothstep(0.0, GONE_AT, height))
