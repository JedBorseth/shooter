extends Node3D
## An explosion (a frag grenade's, or anything else's), built like a frag going
## off on dirt or concrete rather than a fuel fireball:
## - a white-hot flash for a sixth of a second (the flipbook's first,
##   colourless frames, burning white) under a short orange core (from its
##   burning frames on, at a bounded brightness) that is gone in about half a
##   second;
## - jets of dark dirt and dust thrown up out of the ground in a fan
##   (`shaders/dirt_burst.gdshader`, a Blender-rendered flipbook), tearing into
##   clumps and thinning;
## - a brown-grey column of dust and smoke that rises 3 to 6 m and hangs for
##   about ten seconds, thinning as it drifts, and a ring of dust rolled out
##   along the ground (billows from the smoke grenade's flipbook);
## - chips of rubble (`assets/effects/chip.glb`) thrown high, landing and
##   resting on the floor;
## - a few short sparks and a flash of warm light that lights the ground and
##   walls round it for about a fifth of a second.
## About 22 quads and 36 chips on High, fewer on Low and Medium (`DETAIL`).
## Pooled and placed by `CombatEffects.explosion` (`rust/game/src/effects.rs`),
## which also leaves the scorch mark and shakes the camera; the bang plays
## from whoever calls it.

## Seconds the flash takes to fade, and its peak energy.
const FLASH_TIME := 0.22
const FLASH_ENERGY := 18.0
const CHIP_MODEL := "res://assets/effects/chip.glb"
## Per preset (Low, Medium, High): dirt jets, dust billows rising, dust
## billows rolled along the ground, and chips. The large, soft quads cost per
## pixel they cover, so Low draws fewer of them.
const DETAIL := {
	&"Dirt": [5, 6, 6],
	&"Column": [5, 6, 7],
	&"Ring": [6, 7, 7],
	&"Debris": [24, 30, 36],
}

## The chip mesh, taken once from its model and shared by every explosion.
static var _chip: Mesh

var _flash_left := 0.0

@onready
var _emitters: Array[GPUParticles3D] = [$Fireball, $Core, $Dirt, $Column, $Ring, $Debris, $Sparks]
@onready var _flash: OmniLight3D = $Flash


## The mesh debris chips draw: the one in `chip.glb`, loaded on first use.
static func chip_mesh() -> Mesh:
	if _chip == null:
		var scene := load(CHIP_MODEL) as PackedScene
		if scene != null:
			var root := scene.instantiate()
			var found := root.find_children("*", "MeshInstance3D", true, false)
			if not found.is_empty():
				_chip = (found[0] as MeshInstance3D).mesh
			root.free()
	return _chip


## Plays the explosion on a surface facing `normal` (up for the ground), with
## chips landing on the floor at height `floor_y`, at `size` times a frag's.
func setup(normal: Vector3 = Vector3.UP, floor_y := -1000.0, size := 1.0) -> void:
	var up := normal.normalized() if normal.length_squared() > 0.0 else Vector3.UP
	# Smoke rises and dust spreads along the ground whatever the surface, so
	# only lean the burst a little toward a wall's normal.
	up = (up * 0.4 + Vector3.UP * 0.6).normalized()
	var reference := Vector3.FORWARD if absf(up.dot(Vector3.FORWARD)) < 0.99 else Vector3.RIGHT
	var x := reference.cross(up).normalized()
	global_basis = Basis(x, up, x.cross(up)).scaled(Vector3.ONE * size)
	($Debris.process_material as ShaderMaterial).set_shader_parameter(&"floor_y", floor_y)
	# Dirt and dust fade in above the ground the blast went off on (or just
	# below the blast when there is no floor under it).
	var ground := floor_y if floor_y > -999.0 else global_position.y - 0.3
	for emitter: GPUParticles3D in [$Dirt, $Column, $Ring]:
		emitter.set_instance_shader_parameter(&"ground_y", ground)
	var quality := _quality()
	for emitter_name: StringName in DETAIL:
		var emitter := get_node(NodePath(emitter_name)) as GPUParticles3D
		emitter.amount_ratio = float(DETAIL[emitter_name][quality]) / float(emitter.amount)
	for emitter in _emitters:
		emitter.restart()
	_flash.visible = true
	_flash.light_energy = FLASH_ENERGY
	_flash_left = FLASH_TIME
	set_process(true)


func _ready() -> void:
	$Debris.draw_pass_1 = chip_mesh()
	set_process(false)


func _quality() -> int:
	var settings := get_node_or_null(^"/root/Settings")
	if settings == null:
		return 1
	return clampi(int(settings.get(&"graphics_quality")), 0, 2)


func _process(delta: float) -> void:
	_flash_left -= delta
	if _flash_left <= 0.0:
		_flash.visible = false
		set_process(false)
		return
	var t := _flash_left / FLASH_TIME
	_flash.light_energy = FLASH_ENERGY * t * t
