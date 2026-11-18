extends Node
## A key light for the first-person weapon and arms, the way a shooter lights
## its viewmodel. It is not a light node: a second directional light costs every
## lit pixel of the world (0.16 ms on Medium in the Testing Area), so the
## viewmodel shader (shaders/viewmodel.gdshader) adds the key itself, on the
## weapon's own pixels only, from two global shader parameters this node sets:
## `viewmodel_key` (the direction toward the key in world space, and its
## strength) and `viewmodel_key_color`.
##
## The map's sun already lights the weapon when it shines from behind the
## player. When the player faces into the sun, the sun lights only the far side
## of the weapon, and the side the player sees would fall to the flat sky light.
## So the key takes the sun's colour and height, swings round over the
## player's shoulder, and grows as the player turns into the sun. It is weaker
## in shade: one ray toward the sun, a few times a second, tells whether the
## player stands in sunlight.
##
## `Settings` creates one for the whole game (none on a dedicated server).

## The key's strength, as a fraction of the sun's, facing straight into the
## sun in the open, and with the sun straight behind.
const INTO_SUN := 0.4
const SUN_BEHIND := 0.06
## How much of the key is left in shade.
const IN_SHADE := 0.35
## The lowest the key comes from, as the sine of its height above the horizon:
## a sun on the horizon still lights the weapon from a little above.
const MIN_HEIGHT := 0.35
## Rays toward the sun per second, and how fast the key follows them.
const SUN_TESTS := 6.0
const FOLLOW_RATE := 3.0
const SUN_REACH := 2000.0
## Physics layer of the level geometry (project.godot: "world").
const WORLD_LAYER := 1

var _sun: DirectionalLight3D
var _sunlit := 1.0
var _sunlit_target := 1.0
var _test_wait := 0.0
var _strength := -1.0


func _init() -> void:
	name = "ViewmodelLight"
	process_mode = Node.PROCESS_MODE_ALWAYS


func _process(delta: float) -> void:
	var viewport := get_viewport()
	var camera := viewport.get_camera_3d() if viewport != null else null
	if camera == null or not _find_sun():
		_set_key(Vector3.UP, Color.BLACK, 0.0)
		return
	var to_sun := _sun.global_basis.z.normalized()
	var forward := -camera.global_basis.z.normalized()

	_test_wait -= delta
	if _test_wait <= 0.0:
		_test_wait = 1.0 / SUN_TESTS
		_sunlit_target = 0.0 if _sun_hidden(camera.global_position, to_sun) else 1.0
	_sunlit = move_toward(_sunlit, _sunlit_target, delta * FOLLOW_RATE)

	var strength := key_strength(to_sun.dot(forward), _sunlit) * _sun.light_energy
	_set_key(key_direction(to_sun, forward), _sun.light_color, strength)


## Where the key comes from: the sun, swung round to the player's side of the
## weapon as the player faces into it, and never lower than MIN_HEIGHT.
static func key_direction(to_sun: Vector3, forward: Vector3) -> Vector3:
	var key := (to_sun - forward * 2.0 * maxf(to_sun.dot(forward), 0.0)).normalized()
	if key.y >= MIN_HEIGHT:
		return key
	var flat := Vector3(key.x, 0.0, key.z)
	flat = flat.normalized() if flat.length_squared() > 1e-6 else Vector3.BACK
	return flat * sqrt(1.0 - MIN_HEIGHT * MIN_HEIGHT) + Vector3.UP * MIN_HEIGHT


## The key's strength as a fraction of the sun's: `facing` is how far the
## player faces into the sun (-1 to 1), `sunlit` how much they stand in it.
static func key_strength(facing: float, sunlit: float) -> float:
	var strength := lerpf(SUN_BEHIND, INTO_SUN, smoothstep(-0.3, 0.7, facing))
	return strength * lerpf(IN_SHADE, 1.0, sunlit)


func _set_key(direction: Vector3, color: Color, strength: float) -> void:
	if is_zero_approx(strength) and is_zero_approx(_strength):
		return
	_strength = strength
	var key := Vector4(direction.x, direction.y, direction.z, strength)
	RenderingServer.global_shader_parameter_set(&"viewmodel_key", key)
	RenderingServer.global_shader_parameter_set(&"viewmodel_key_color", color)


func _sun_hidden(from: Vector3, to_sun: Vector3) -> bool:
	var space := get_viewport().world_3d.direct_space_state
	var query := PhysicsRayQueryParameters3D.create(from, from + to_sun * SUN_REACH, WORLD_LAYER)
	return not space.intersect_ray(query).is_empty()


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
