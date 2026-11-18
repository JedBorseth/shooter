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
## player's shoulder, and grows as the player turns into the sun. One ray
## toward the sun, a few times a second, tells whether the player stands in
## sunlight.
##
## In shade the sun no longer reaches the weapon (it takes the sun's shadow
## like the world does), and a black rifle lit only by the probes' soft light
## goes nearly black while the picture is exposed for the sunlit street
## beyond. So in shade the key becomes the sky: a light from above and ahead,
## in the sky's colour, as strong as the map's sky light (its ambient
## energy), which brings back the gloves and the sheen along the top of the
## receiver and the rail. Under a roof (a second ray, straight up) it drops to
## a quarter: the room's own light, baked into the lightmap's probes, lights
## the weapon there. The arms take the key too (a first-person copy of the
## character shader, `viewmodel::first_person_shader`).
##
## `Settings` creates one for the whole game (none on a dedicated server).

## The key's strength, as a fraction of the sun's, facing straight into the
## sun in the open, and with the sun straight behind.
const INTO_SUN := 0.4
const SUN_BEHIND := 0.06
## The sky's fill in shade, as a multiple of the map's sky light (ambient
## energy), under open sky and under a roof.
const SKY_FILL := 1.8
const ROOF_FILL := 0.45
## Where the sky's fill comes from: mostly overhead, tipped ahead of the
## player, so the top of the weapon catches it toward the eye.
const SKY_AHEAD := 0.6
## The sky's colour for maps lit by their sky, blended toward the sun's so a
## dusk sky stays warm. Maps lit by a flat ambient colour use that colour.
const SKY_TINT := Color(0.58, 0.7, 1.0)
const SKY_SUN_SHARE := 0.3
## The lowest the key comes from, as the sine of its height above the horizon:
## a sun on the horizon still lights the weapon from a little above.
const MIN_HEIGHT := 0.35
## Rays toward the sun per second, and how fast the key follows them.
const SUN_TESTS := 6.0
const FOLLOW_RATE := 3.0
const SUN_REACH := 2000.0
const ROOF_REACH := 30.0
## Physics layer of the level geometry (project.godot: "world").
const WORLD_LAYER := 1

var _sun: DirectionalLight3D
var _sunlit := 1.0
var _sunlit_target := 1.0
var _open := 1.0
var _open_target := 1.0
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
		var eye := camera.global_position
		_sunlit_target = 0.0 if _hidden(eye, to_sun * SUN_REACH) else 1.0
		_open_target = 0.0 if _hidden(eye, Vector3.UP * ROOF_REACH) else 1.0
	_sunlit = move_toward(_sunlit, _sunlit_target, delta * FOLLOW_RATE)
	_open = move_toward(_open, _open_target, delta * FOLLOW_RATE)

	var sun := key_strength(to_sun.dot(forward), _sunlit) * _sun.light_energy
	var sky_color := SKY_TINT.lerp(_sun.light_color, SKY_SUN_SHARE)
	var sky_light := 1.0
	var environment := _environment(camera)
	if environment != null:
		sky_light = environment.ambient_light_energy
		if environment.ambient_light_source == Environment.AMBIENT_SOURCE_COLOR:
			var ambient := environment.ambient_light_color
			sky_light *= ambient.get_luminance()
			sky_color = ambient / maxf(ambient.get_luminance(), 1e-3)
	var sky := sky_strength(_sunlit, _open) * sky_light
	var key := blend(
		key_direction(to_sun, forward),
		_sun.light_color,
		sun,
		sky_direction(forward),
		sky_color,
		sky
	)
	_set_key(key[0], key[1], key[2])


## Where the key comes from: the sun, swung round to the player's side of the
## weapon as the player faces into it, and never lower than MIN_HEIGHT.
static func key_direction(to_sun: Vector3, forward: Vector3) -> Vector3:
	var key := (to_sun - forward * 2.0 * maxf(to_sun.dot(forward), 0.0)).normalized()
	if key.y >= MIN_HEIGHT:
		return key
	var flat := Vector3(key.x, 0.0, key.z)
	flat = flat.normalized() if flat.length_squared() > 1e-6 else Vector3.BACK
	return flat * sqrt(1.0 - MIN_HEIGHT * MIN_HEIGHT) + Vector3.UP * MIN_HEIGHT


## The sun key's strength as a fraction of the sun's: `facing` is how far
## the player faces into the sun (-1 to 1), `sunlit` how much they stand in
## it. In shade the sun has no part in it.
static func key_strength(facing: float, sunlit: float) -> float:
	return lerpf(SUN_BEHIND, INTO_SUN, smoothstep(-0.3, 0.7, facing)) * sunlit


## The sky fill's strength as a multiple of the map's sky light: none in
## sunlight, SKY_FILL in shade under open sky, ROOF_FILL under a roof.
static func sky_strength(sunlit: float, open: float) -> float:
	return lerpf(ROOF_FILL, SKY_FILL, open) * (1.0 - sunlit)


## Where the sky fill comes from for a player looking along `forward`.
static func sky_direction(forward: Vector3) -> Vector3:
	var ahead := Vector3(forward.x, 0.0, forward.z)
	ahead = ahead.normalized() if ahead.length_squared() > 1e-6 else Vector3.FORWARD
	return (Vector3.UP + ahead * SKY_AHEAD).normalized()


## The shader draws one key, so the sun's and the sky's are blended: their
## directions and colours weighted by strength, and the strengths summed.
## Returns [direction, colour, strength].
static func blend(
	sun_dir: Vector3,
	sun_color: Color,
	sun: float,
	sky_dir: Vector3,
	sky_color: Color,
	sky: float,
) -> Array:
	var total := sun + sky
	if total <= 0.0:
		return [sun_dir, sun_color, 0.0]
	var w := sky / total
	var direction := sun_dir.lerp(sky_dir, w)
	direction = direction.normalized() if direction.length_squared() > 1e-6 else sky_dir
	return [direction, sun_color.lerp(sky_color, w), total]


func _set_key(direction: Vector3, color: Color, strength: float) -> void:
	if is_zero_approx(strength) and is_zero_approx(_strength):
		return
	_strength = strength
	var key := Vector4(direction.x, direction.y, direction.z, strength)
	RenderingServer.global_shader_parameter_set(&"viewmodel_key", key)
	RenderingServer.global_shader_parameter_set(&"viewmodel_key_color", color)


func _hidden(from: Vector3, toward: Vector3) -> bool:
	var space := get_viewport().world_3d.direct_space_state
	var query := PhysicsRayQueryParameters3D.create(from, from + toward, WORLD_LAYER)
	return not space.intersect_ray(query).is_empty()


func _environment(camera: Camera3D) -> Environment:
	if camera.environment != null:
		return camera.environment
	var world := camera.get_world_3d()
	return world.environment if world != null else null


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
