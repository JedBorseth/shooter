extends Node3D
## A power-up dropped by a zombie (`ZombieDirector` spawns and collects it):
## a glowing token that spins and bobs over the ground, with its name above
## it, and blinks in its last seconds. Each kind has its own shape.

const GLOW := Color(0.45, 1.0, 0.35)
## Seconds left when the token starts to blink.
const BLINK_FROM := 5.0
const SHAPES := {
	"max_ammo": "box",
	"insta_kill": "skull",
	"double_points": "x2",
	"nuke": "bomb",
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
	var material := StandardMaterial3D.new()
	material.albedo_color = GLOW.darkened(0.4)
	material.emission_enabled = true
	material.emission = GLOW
	material.emission_energy_multiplier = 2.2
	material.metallic = 0.6
	material.roughness = 0.3
	_token = Node3D.new()
	add_child(_token)
	match SHAPES.get(kind, "box"):
		"box":
			_add_mesh(_box(Vector3(0.5, 0.32, 0.26)), material, Vector3.ZERO)
			_add_mesh(_box(Vector3(0.3, 0.06, 0.08)), material, Vector3(0, 0.2, 0))
		"skull":
			var sphere := SphereMesh.new()
			sphere.radius = 0.2
			sphere.height = 0.36
			_add_mesh(sphere, material, Vector3(0, 0.04, 0))
			_add_mesh(_box(Vector3(0.22, 0.12, 0.2)), material, Vector3(0, -0.16, 0.02))
		"x2":
			var text := Label3D.new()
			text.text = "x2"
			text.font_size = 160
			text.pixel_size = 0.004
			text.outline_size = 24
			text.modulate = GLOW
			text.outline_modulate = Color(0.05, 0.2, 0.05)
			text.double_sided = true
			_token.add_child(text)
		"bomb":
			var body := CylinderMesh.new()
			body.top_radius = 0.16
			body.bottom_radius = 0.16
			body.height = 0.42
			var shell := MeshInstance3D.new()
			shell.mesh = body
			shell.material_override = material
			shell.rotation = Vector3(0, 0, PI / 2)
			_token.add_child(shell)
			var cap := SphereMesh.new()
			cap.radius = 0.16
			cap.height = 0.32
			_add_mesh(cap, material, Vector3(0.21, 0, 0))
			_add_mesh(_box(Vector3(0.04, 0.24, 0.24)), material, Vector3(-0.24, 0, 0))
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


func _box(size: Vector3) -> BoxMesh:
	var box := BoxMesh.new()
	box.size = size
	return box


func _add_mesh(mesh: Mesh, material: Material, offset: Vector3) -> void:
	var instance := MeshInstance3D.new()
	instance.mesh = mesh
	instance.material_override = material
	instance.position = offset
	instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_token.add_child(instance)
