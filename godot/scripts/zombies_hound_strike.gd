extends Node3D
## The strike that brings a Zombies hound in (`ZombiesEvent::HoundStrike`,
## `net/horde_view.rs`): for 1.5 s a cold light crackles on the spot, then a
## bolt falls from the sky and the hound stands there (the server spawns it
## then; `shooter_sim::zombies::hounds::STRIKE_SECONDS`). Black Ops' dog
## spawn lightning, in the hounds' own cold blue-white rather than its
## orange. An effect, so its bolt is a plain unshaded quad, not a model.

const GLOW := Color(0.62, 0.86, 1.0)
## Seconds of crackle before the bolt (the server's strike delay).
const CRACKLE := 1.5
## Seconds the bolt and its flash last.
const BOLT := 0.35
const BOLT_HEIGHT := 40.0

var _age := 0.0
var _light: OmniLight3D
var _bolt: MeshInstance3D


func _ready() -> void:
	_light = OmniLight3D.new()
	_light.light_color = GLOW
	_light.omni_range = 6.0
	_light.light_energy = 0.0
	_light.shadow_enabled = false
	_light.position = Vector3(0, 0.6, 0)
	add_child(_light)
	_bolt = MeshInstance3D.new()
	var quad := QuadMesh.new()
	quad.size = Vector2(0.35, BOLT_HEIGHT)
	_bolt.mesh = quad
	var material := StandardMaterial3D.new()
	material.shading_mode = BaseMaterial3D.SHADING_MODE_UNSHADED
	material.billboard_mode = BaseMaterial3D.BILLBOARD_FIXED_Y
	material.transparency = BaseMaterial3D.TRANSPARENCY_ALPHA
	material.blend_mode = BaseMaterial3D.BLEND_MODE_ADD
	material.albedo_color = Color(GLOW.r * 3.0, GLOW.g * 3.0, GLOW.b * 3.0, 1.0)
	_bolt.material_override = material
	_bolt.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	_bolt.position = Vector3(0, BOLT_HEIGHT * 0.5, 0)
	_bolt.visible = false
	add_child(_bolt)


func _process(delta: float) -> void:
	_age += delta
	if _age < CRACKLE:
		# A flicker that builds toward the bolt.
		var build := _age / CRACKLE
		var flicker := 0.5 + 0.5 * sin(_age * 53.0) * sin(_age * 31.0)
		_light.light_energy = build * 2.5 * flicker
		return
	var t := (_age - CRACKLE) / BOLT
	if t >= 1.0:
		queue_free()
		return
	_bolt.visible = t < 0.6 and fmod(t * 6.0, 1.0) < 0.75
	_light.omni_range = 14.0
	_light.light_energy = 12.0 * (1.0 - t)
