extends Node3D
## Search and Destroy's charge going off: much bigger than a frag, as Call of
## Duty's bomb is. `CombatEffects.charge_explosion` plays the frag's
## explosion scaled up under it (dirt, dust, chips, sparks, scorch, and the
## camera shake), and this adds what only the charge has:
## - a fireball of flipbook billows, 6.5 m across, rolling up about 6 m
##   and burning out in about a second and a half;
## - a dark smoke column fed for about 5 s, rising to about 15 m and gone
##   after about 10 s;
## - a skirt of dust rolled out along the ground to about 10 m;
## - a flash that lights 26 m round for a third of a second.
## Pooled (one) and placed by `CombatEffects`. The sound plays from
## `snd_view.gd`'s charge audio when it is there, else this plays the frag's
## distant bang, louder.

const FLASH_TIME := 0.35
const FLASH_ENERGY := 14.0
## Per preset (Low, Medium, High): fireball billows, smoke column billows,
## and ground skirt billows. Big soft quads cost per pixel, so Low draws
## fewer.
const DETAIL := {
	&"Fireball": [8, 11, 14],
	&"Plume": [16, 22, 28],
	&"Skirt": [8, 11, 14],
}
## The charge's own explosion sound; until it exists, the frag's bang.
const OWN_SOUND := "res://assets/audio/world/charge_explosion.ogg"
const FALLBACK_SOUNDS := [
	"res://assets/audio/ambience/explosion_far_1.ogg",
	"res://assets/audio/ambience/explosion_far_2.ogg",
	"res://assets/audio/ambience/explosion_far_3.ogg",
]

var _flash_left := 0.0

@onready var _emitters: Array[GPUParticles3D] = [$Fireball, $Plume, $Skirt]
@onready var _flash: OmniLight3D = $Flash


## Plays the blast with its dust fading in above the floor at `floor_y`.
func setup(floor_y := -1000.0) -> void:
	var ground := floor_y if floor_y > -999.0 else global_position.y - 0.3
	for emitter: GPUParticles3D in [$Plume, $Skirt]:
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
	if not ResourceLoader.exists(OWN_SOUND):
		_bang()


func _ready() -> void:
	set_process(false)


func _quality() -> int:
	var settings := get_node_or_null(^"/root/Settings")
	if settings == null:
		return 1
	return clampi(int(settings.get(&"graphics_quality")), 0, 2)


func _bang() -> void:
	var player := AudioStreamPlayer3D.new()
	player.stream = load(FALLBACK_SOUNDS[randi() % FALLBACK_SOUNDS.size()])
	player.bus = &"Effects"
	player.volume_db = 14.0
	player.unit_size = 30.0
	player.max_db = 6.0
	player.attenuation_filter_cutoff_hz = 6000.0
	player.attenuation_filter_db = -12.0
	add_child(player)
	player.play()
	player.finished.connect(player.queue_free)


func _process(delta: float) -> void:
	_flash_left -= delta
	if _flash_left <= 0.0:
		_flash.visible = false
		set_process(false)
		return
	var t := _flash_left / FLASH_TIME
	_flash.light_energy = FLASH_ENERGY * t * t
