extends Node3D
## A power-up dropped by a zombie (`ZombieDirector` spawns and collects it):
## a glowing token that spins and bobs over the ground, with its name above
## it, and blinks in its last seconds. Each kind has its own shape.

const GLOW := Color(0.45, 1.0, 0.35)
## Seconds left when the token starts to blink.
const BLINK_FROM := 5.0
## Each kind's model (tools/gear_models.py, built in Blender): an ammo can,
## a skull, "x2", and a bomb, glowing.
const MODELS := {
	"max_ammo": preload("res://assets/models/powerup_ammo/powerup_ammo.gltf"),
	"insta_kill": preload("res://assets/models/powerup_skull/powerup_skull.gltf"),
	"double_points": preload("res://assets/models/powerup_x2/powerup_x2.gltf"),
	"nuke": preload("res://assets/models/powerup_bomb/powerup_bomb.gltf"),
}
const LABELS := {
	"max_ammo": "MAX AMMO",
	"insta_kill": "INSTA-KILL",
	"double_points": "DOUBLE POINTS",
	"nuke": "KABOOM",
}

var kind := ""
var _time_left := 30.0
var _age := 0.0
var _token: Node3D
var _light: OmniLight3D


## Builds the token for a power-up key such as "max_ammo".
func setup(power_up: String) -> void:
	kind = power_up
	_token = Node3D.new()
	add_child(_token)
	var scene := MODELS.get(kind, MODELS["max_ammo"]) as PackedScene
	var model := scene.instantiate() as Node3D
	for mesh: Node in model.find_children("*", "MeshInstance3D", true, false):
		(mesh as MeshInstance3D).cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_token.add_child(model)
	_light = OmniLight3D.new()
	_light.light_color = GLOW
	_light.light_energy = 1.6
	_light.omni_range = 4.0
	add_child(_light)
	var label := Label3D.new()
	label.text = LABELS.get(kind, kind.to_upper())
	label.font_size = 48
	label.pixel_size = 0.005
	label.outline_size = 10
	label.modulate = GLOW
	label.billboard = BaseMaterial3D.BILLBOARD_ENABLED
	label.position = Vector3(0, 0.55, 0)
	label.no_depth_test = true
	add_child(label)


## Called each tick with the seconds before the token vanishes.
func set_time_left(seconds: float) -> void:
	_time_left = seconds


func _process(delta: float) -> void:
	_age += delta
	if _token == null:
		return
	_token.rotation.y += delta * 2.0
	_token.position.y = sin(_age * 2.4) * 0.08
	# Blink faster and faster as it is about to vanish.
	if _time_left < BLINK_FROM:
		var rate := lerpf(10.0, 3.0, _time_left / BLINK_FROM)
		visible = fmod(_age * rate, 1.0) < 0.6
	else:
		visible = true
