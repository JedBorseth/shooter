class_name SecretLaser
extends Node3D
## Nuketown easter egg. One stretch of wall near the spawn is not quite solid: a
## faint pink triangle marks it. Walk into it and keep pushing for a moment to
## phase through into a hidden vault holding the Prism Laser. The glowing ring
## in the vault sends you back.
##
## The map is converted render geometry with no hand-placed gameplay markers, so
## the wall is found at runtime: the first upright surface a ray hits looking
## back from the player's spawn, sweeping outward until one fits. The same spawn
## and map give the same wall every time.

## Emitted on every phase through the wall, both ways.
signal phased(into_vault: bool)
## Emitted the first time the player takes the laser.
signal laser_taken

const WEAPON := "laser"
const WORLD_LAYER := 1
## Rays from the spawn look this far for the secret wall.
const SEARCH_DISTANCE := 35.0
## Walls closer to the spawn than this are skipped, so it isn't found by accident.
const MIN_DISTANCE := 3.0
const CHEST_HEIGHT := 1.2
## Seconds of pushing into the wall before it gives way.
const PUSH_SECONDS := 0.8
## How close to the wall (metres from its surface to the player's centre) and
## how far along it counts as pushing into the marked spot.
const REACH := 0.9
const HALF_WIDTH := 0.9
## Seconds after a phase before either portal works again.
const COOLDOWN := 1.0
## The vault floats this far above the spawn, out of sight of the map.
const VAULT_LIFT := 120.0
const VAULT_SIZE := Vector3(8.0, 4.0, 8.0)
## The vault's room and the laser's pedestal, built in Blender (tools/gear_models.py).
const VAULT_MODEL := preload("res://assets/models/laser_vault/laser_vault.gltf")
const PEDESTAL_MODEL := preload("res://assets/models/laser_pedestal/laser_pedestal.gltf")
const GLOW := Color(1.0, 0.18, 0.82)

const GLYPH_SHADER := """
shader_type spatial;
render_mode blend_add, unshaded, cull_disabled, depth_draw_never, shadows_disabled;

uniform vec4 color : source_color = vec4(1.0, 0.18, 0.82, 1.0);
uniform float charge = 0.0;

float triangle(vec2 p, float r) {
	const float k = sqrt(3.0);
	p.x = abs(p.x) - r;
	p.y = p.y + r / k;
	if (p.x + k * p.y > 0.0) {
		p = vec2(p.x - k * p.y, -k * p.x - p.y) / 2.0;
	}
	p.x -= clamp(p.x, -2.0 * r, 0.0);
	return -length(p) * sign(p.y);
}

void fragment() {
	vec2 p = vec2(UV.x * 2.0 - 1.0, 1.0 - UV.y * 2.0);
	float d = triangle(p, 0.62);
	float line = smoothstep(0.05, 0.0, abs(d));
	float core = smoothstep(0.16, 0.0, length(p - vec2(0.0, -0.05)));
	float pulse = 0.5 + 0.5 * sin(TIME * 2.2);
	float strength = 0.1 + 0.08 * pulse + charge * 2.5;
	float fill = smoothstep(0.0, -0.4, d) * charge;
	ALBEDO = color.rgb * (line + core * 0.6 + fill) * strength;
}
"""

@export var player_path: NodePath = ^"../Player"
@export var laser_model: PackedScene = preload("res://scenes/weapons/laser_model.tscn")

## Where the marked spot is: on the wall surface at chest height, and its normal.
var wall_point := Vector3.ZERO
var wall_normal := Vector3.ZERO
var found := false
var in_vault := false

var _anchor := Vector3.ZERO
var _yaw := 0.0
var _searches_left := 0
var _floor_y := 0.0
var _push := 0.0
var _cooldown := 0.0
var _laser_taken := false
var _glyph_material: ShaderMaterial
var _vault: Node3D
var _display: Node3D
var _phase_sound: AudioStreamPlayer
var _flash: ColorRect
var _time := 0.0

@onready var _player: FpsPlayer = get_node_or_null(player_path) as FpsPlayer


## Hides the secret near a spawn point. The wall search runs on the next
## physics frames, once the map's collision is in the physics space.
func place(anchor: Vector3, yaw: float) -> void:
	_anchor = anchor
	_yaw = yaw
	_searches_left = 10


## Where the player lands in the vault.
func vault_spawn() -> Vector3:
	return _vault_origin() + Vector3(0.0, 0.05, VAULT_SIZE.z * 0.5 - 2.0)


## Where the exit ring sends the player: just in front of the secret wall.
func return_point() -> Vector3:
	var point := wall_point + wall_normal * 1.2
	point.y = _floor_y + 0.05
	return point


func enter_vault() -> void:
	if _vault == null:
		return
	in_vault = true
	_phase(vault_spawn(), 0.0)
	phased.emit(true)


func leave_vault() -> void:
	in_vault = false
	# Face away from the wall, back into the map.
	_phase(return_point(), atan2(-wall_normal.x, -wall_normal.z))
	phased.emit(false)


func _ready() -> void:
	_phase_sound = AudioStreamPlayer.new()
	_phase_sound.stream = load("res://assets/audio/player/phase.wav")
	_phase_sound.volume_db = -4.0
	_phase_sound.bus = &"Effects"
	add_child(_phase_sound)
	var layer := CanvasLayer.new()
	layer.layer = 10
	add_child(layer)
	_flash = ColorRect.new()
	_flash.color = Color(GLOW, 0.0)
	_flash.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_flash.set_anchors_preset(Control.PRESET_FULL_RECT)
	layer.add_child(_flash)


func _physics_process(delta: float) -> void:
	if _searches_left > 0:
		_searches_left -= 1
		found = _find_wall()
		if found:
			_searches_left = 0
			_build_glyph()
			_build_vault()
		elif _searches_left == 0:
			push_warning("SecretLaser: no wall near the spawn to hide the vault behind.")
	if not found or _player == null:
		return
	_cooldown = maxf(_cooldown - delta, 0.0)
	if not _laser_taken and _player.weapon_key() == WEAPON:
		_laser_taken = true
		laser_taken.emit()
	if in_vault:
		_check_exit()
	else:
		_check_push(delta)


func _process(delta: float) -> void:
	_time += delta
	if _display != null:
		_display.rotation.y = _time * 1.4
		_display.position.y = 1.45 + sin(_time * 2.0) * 0.06
	if _flash != null and _flash.color.a > 0.0:
		_flash.color.a = maxf(_flash.color.a - delta * 1.8, 0.0)


func _find_wall() -> bool:
	var space := get_world_3d().direct_space_state
	var origin := _anchor + Vector3.UP * CHEST_HEIGHT
	var back := Basis(Vector3.UP, _yaw) * Vector3.BACK
	# Behind the spawn first, then sweep outward on both sides.
	for i in 16:
		var side := 1.0 if i % 2 == 1 else -1.0
		var angle := ceilf(i / 2.0) * side * TAU / 16.0
		var direction := back.rotated(Vector3.UP, angle)
		var hit := _ray(space, origin, origin + direction * SEARCH_DISTANCE)
		if hit.is_empty() or not _usable_wall(space, origin, direction, hit):
			continue
		wall_point = hit["position"]
		wall_normal = hit["normal"]
		wall_normal = Vector3(wall_normal.x, 0.0, wall_normal.z).normalized()
		return true
	return false


## A wall is usable when it is upright, far enough from the spawn, tall enough
## to hide a doorway, and has floor in front of it to stand on.
func _usable_wall(
	space: PhysicsDirectSpaceState3D, origin: Vector3, direction: Vector3, hit: Dictionary
) -> bool:
	var point: Vector3 = hit["position"]
	var normal: Vector3 = hit["normal"]
	var distance := origin.distance_to(point)
	if distance < MIN_DISTANCE or absf(normal.y) > 0.25:
		return false
	var high := origin + Vector3.UP * 0.7
	var upper := _ray(space, high, high + direction * SEARCH_DISTANCE)
	if upper.is_empty() or absf(high.distance_to(upper["position"]) - distance) > 0.4:
		return false
	var stand := point + Vector3(normal.x, 0.0, normal.z).normalized() * 0.6
	var floor_hit := _ray(space, stand, stand + Vector3.DOWN * 3.0)
	if floor_hit.is_empty():
		return false
	var ground: Vector3 = floor_hit["position"]
	_floor_y = ground.y
	return true


func _ray(space: PhysicsDirectSpaceState3D, from: Vector3, to: Vector3) -> Dictionary:
	var query := PhysicsRayQueryParameters3D.create(from, to, WORLD_LAYER)
	if _player != null:
		query.exclude = [_player.get_rid()]
	return space.intersect_ray(query)


func _check_push(delta: float) -> void:
	var feet := _player.global_position
	var offset := feet - Vector3(wall_point.x, _floor_y, wall_point.z)
	var depth := offset.dot(wall_normal)
	var along := offset - wall_normal * depth
	along.y = 0.0
	var yaw := _player.view_yaw()
	var forward := Vector3(-sin(yaw), 0.0, -cos(yaw))
	var moving := Input.get_vector("move_left", "move_right", "move_forward", "move_back")
	var pushing := (
		_cooldown <= 0.0
		and depth > -0.5
		and depth < REACH
		and along.length() < HALF_WIDTH
		and absf(feet.y - _floor_y) < 1.0
		and forward.dot(-wall_normal) > 0.5
		and moving.y < -0.5
	)
	if pushing:
		_push += delta
	else:
		_push = maxf(_push - delta * 2.0, 0.0)
	_glyph_material.set_shader_parameter("charge", clampf(_push / PUSH_SECONDS, 0.0, 1.0))
	if _push >= PUSH_SECONDS:
		_push = 0.0
		_glyph_material.set_shader_parameter("charge", 0.0)
		enter_vault()


func _check_exit() -> void:
	var exit := _vault_origin() + Vector3(0.0, 0.0, VAULT_SIZE.z * 0.5 - 0.6)
	var offset := _player.global_position - exit
	if _cooldown <= 0.0 and Vector2(offset.x, offset.z).length() < 0.8:
		leave_vault()


func _phase(position: Vector3, yaw: float) -> void:
	_cooldown = COOLDOWN
	_player.teleport(position, yaw)
	_phase_sound.play()
	_flash.color.a = 0.85


func _vault_origin() -> Vector3:
	return _anchor + Vector3.UP * VAULT_LIFT


func _build_glyph() -> void:
	var shader := Shader.new()
	shader.code = GLYPH_SHADER
	_glyph_material = ShaderMaterial.new()
	_glyph_material.shader = shader
	var quad := QuadMesh.new()
	quad.size = Vector2(0.9, 0.9)
	quad.material = _glyph_material
	var glyph := MeshInstance3D.new()
	glyph.name = "Glyph"
	glyph.mesh = quad
	glyph.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	add_child(glyph)
	# A QuadMesh faces +Z; turn it to face out of the wall.
	var point := wall_point + wall_normal * 0.02
	glyph.global_transform = Transform3D(Basis.looking_at(-wall_normal), point)


func _build_vault() -> void:
	_vault = Node3D.new()
	_vault.name = "Vault"
	add_child(_vault)
	_vault.global_position = _vault_origin()
	var size := VAULT_SIZE
	var t := 0.5
	var shell := StaticBody3D.new()
	shell.name = "Shell"
	shell.set_meta("surface", "metal")
	_vault.add_child(shell)
	# Collision only: the room's look is a model (tools/gear_models.py
	# `laser_vault`), with the glowing strips and the exit ring built in.
	var boxes := [
		[Vector3(0, -t / 2, 0), Vector3(size.x + 2 * t, t, size.z + 2 * t)],
		[Vector3(0, size.y + t / 2, 0), Vector3(size.x + 2 * t, t, size.z + 2 * t)],
		[Vector3(-(size.x + t) / 2, size.y / 2, 0), Vector3(t, size.y, size.z)],
		[Vector3((size.x + t) / 2, size.y / 2, 0), Vector3(t, size.y, size.z)],
		[Vector3(0, size.y / 2, -(size.z + t) / 2), Vector3(size.x, size.y, t)],
		[Vector3(0, size.y / 2, (size.z + t) / 2), Vector3(size.x, size.y, t)],
	]
	for box: Array in boxes:
		_add_solid(shell, box[0], box[1])
	_vault.add_child(VAULT_MODEL.instantiate())
	var light := OmniLight3D.new()
	light.position = Vector3(0, size.y - 0.6, 0)
	light.light_color = Color(1.0, 0.75, 0.95)
	light.light_energy = 2.0
	light.omni_range = 9.0
	_vault.add_child(light)
	_build_pedestal()
	_build_exit()
	_add_label(Vector3(0, 3.6, -size.z / 2 + 0.3), 0.0, "YOU FOUND IT", 96, GLOW)


func _build_pedestal() -> void:
	# WeaponPickup is the same interactable as the weapon racks: look at it and
	# press interact. Its collision covers the floating gun, so aiming at the
	# gun finds it too.
	var pickup := WeaponPickup.new()
	pickup.name = "Pedestal"
	pickup.weapon = WEAPON
	pickup.set_meta("surface", "metal")
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = Vector3(0.7, 1.8, 0.7)
	shape.shape = box
	shape.position = Vector3(0, 0.9, 0)
	pickup.add_child(shape)
	var label := Label3D.new()
	label.name = "Label"
	label.position = Vector3(0, 1.95, 0)
	label.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	label.pixel_size = 0.004
	label.font_size = 40
	label.outline_size = 8
	label.modulate = Color(1.0, 0.7, 0.95)
	pickup.add_child(label)
	pickup.add_child(PEDESTAL_MODEL.instantiate())
	_vault.add_child(pickup)
	_display = laser_model.instantiate() as Node3D
	_display.scale = Vector3.ONE * 2.2
	pickup.add_child(_display)
	var glow := OmniLight3D.new()
	glow.position = Vector3(0, 1.4, 0)
	glow.light_color = GLOW
	glow.light_energy = 2.5
	glow.omni_range = 4.0
	pickup.add_child(glow)


func _build_exit() -> void:
	# The glowing ring and the pad before it are part of the vault's model.
	_add_label(Vector3(0, 2.3, VAULT_SIZE.z / 2 - 0.3), PI, "EXIT", 64, Color.WHITE)


## Collision for a box of the vault's shell.
func _add_solid(parent: Node3D, center: Vector3, size: Vector3) -> void:
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = size
	shape.shape = box
	shape.position = center
	parent.add_child(shape)


func _add_label(position: Vector3, yaw: float, text: String, size: int, color: Color) -> void:
	var label := Label3D.new()
	label.text = text
	label.position = position
	label.rotation.y = yaw
	label.font_size = size
	label.pixel_size = 0.005
	label.outline_size = 12
	label.modulate = color
	_vault.add_child(label)
