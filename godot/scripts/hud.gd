extends CanvasLayer
## Heads-up display: crosshair, hit markers and sounds, score popups and
## medals, ammo with low-ammo warnings, health, damage feedback, scope overlay,
## pickup prompts, the death screen, and a movement readout for tuning.

## Seconds the controls line stays up after spawning; F1 toggles it.
const CONTROLS_TIME := 60.0

const GAMEPAD_CONTROLS := (
	"LS move   L3 sprint (toggle)   B crouch (slide while sprinting)   A jump"
	+ "   RT fire   LT aim   X reload   Y switch   RS look (aim assist on)"
)

@export var player_path: NodePath
## Replaces the keyboard controls line when set (levels without the pause menu).
@export_multiline var controls_text := ""
## Multiplayer kill scoring (popups and medals). Zombies turns it off and shows
## its own points.
@export var score_feed := true

var _controls_left := CONTROLS_TIME
var _controls_forced := false

@onready var _player: FpsPlayer = get_node_or_null(player_path) as FpsPlayer
@onready var _crosshair: Control = $Crosshair
@onready var _readout: Label = $Readout
@onready var _controls: Label = $Controls
@onready var _weapon: Label = $Ammo/Weapon
@onready var _ammo: Label = $Ammo/Count
@onready var _warning: Label = $Warning
@onready var _prompt: Label = $Prompt
@onready var _health: ProgressBar = $Health
@onready var _death: Label = $Death
@onready var _damage: Control = $Damage
@onready var _scope: Control = $Scope
@onready var _hit_sound: AudioStreamPlayer = $HitSound
@onready var _keyboard_controls := controls_text if controls_text != "" else _controls.text
@onready var _score_feed: Control = $ScoreFeed


func _ready() -> void:
	if not score_feed:
		_score_feed.queue_free()
		_score_feed = null
	if _player == null:
		return
	_player.hit_confirmed.connect(_on_hit)
	if _score_feed != null:
		_player.scored.connect(_score_feed.add_award)
		_score_feed.player = _player
	_player.damaged.connect(_damage.on_damaged)
	_player.respawned.connect(func() -> void: _controls_left = 0.0)
	_damage.player = _player
	_scope.player = _player


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("toggle_controls"):
		_controls_forced = not _controls.visible
		_controls_left = 0.0


func _on_hit(killed: bool, headshot: bool) -> void:
	_crosshair.show_hit(killed, headshot)
	var path := (
		"res://assets/audio/impacts/%s.ogg"
		% ("kill" if killed else ("headshot" if headshot else "hitmarker"))
	)
	if ResourceLoader.exists(path):
		_hit_sound.stream = load(path)
		_hit_sound.play()


func _process(delta: float) -> void:
	if _player == null:
		return
	var show_fps: bool = Settings.show_fps
	_readout.visible = show_fps
	_readout.text = (
		"%s   %.1f m/s   %d fps"
		% [
			_player.movement_state(),
			_player.horizontal_speed(),
			Engine.get_frames_per_second(),
		]
	)
	_controls_left = maxf(_controls_left - delta, 0.0)
	_controls.visible = _controls_forced or _controls_left > 0.0
	_controls.text = GAMEPAD_CONTROLS if _player.using_gamepad() else _keyboard_controls

	var alive := _player.is_alive()
	var scoped := _player.scope_amount() >= 0.85
	_weapon.text = _player.weapon_name()
	var magazine := _player.magazine()
	var reserve := _player.reserve()
	var low := magazine <= _player.low_ammo_threshold()
	_ammo.text = "%d / %d" % [magazine, reserve]
	_ammo.modulate = Color(1, 0.35, 0.3) if low else Color.WHITE

	_warning.visible = alive and not scoped
	if _player.is_reloading():
		_warning.text = "RELOADING"
		_warning.modulate = Color(1, 1, 1, 0.8)
	elif magazine == 0 and reserve == 0:
		_warning.text = "NO AMMO"
		_warning.modulate = Color(1, 0.3, 0.25)
	elif low and reserve > 0:
		_warning.text = "[R] RELOAD"
		_warning.modulate = Color(1, 0.85, 0.3)
	elif low:
		_warning.text = "LOW AMMO"
		_warning.modulate = Color(1, 0.5, 0.3)
	else:
		_warning.visible = false

	_prompt.text = _player.interact_prompt()
	_prompt.visible = alive and _prompt.text != ""

	_health.max_value = _player.max_health()
	_health.value = _player.health()
	_health.modulate.a = 1.0 if _player.health() < _player.max_health() else 0.35

	# Without respawns (Zombies) the mode shows its own end screen.
	_death.visible = not alive and _player.auto_respawn
	if _death.visible:
		_death.text = "YOU DIED\nRespawning in %.1f" % _player.respawn_time_left()

	# Convert the spread cone to pixels so the crosshair shows where shots can land.
	# Shotguns add their pellet cone, so the crosshair matches the pattern.
	var half_fov := deg_to_rad(_player.camera_fov()) * 0.5
	var pixels_per_unit := get_viewport().get_visible_rect().size.y * 0.5 / tan(half_fov)
	var spread := _player.spread_radians()
	if _player.pellet_count() > 1:
		spread += _player.pellet_spread_radians()
	_crosshair.gap = tan(spread) * pixels_per_unit
	_crosshair.alpha = 0.0 if not alive else 1.0 - clampf(_player.ads_amount() * 1.5, 0.0, 1.0)
