extends CanvasLayer
## Heads-up display: crosshair, hit markers and sounds, score popups and
## medals, ammo with low-ammo warnings, health, damage feedback, scope overlay,
## pickup prompts, the death screen, and a movement readout for tuning.
##
## Health sits bottom left and the ammo cluster (weapon silhouette and name, big
## magazine count, reserve, magazine bar) bottom right, each on a soft shade.
## Prompts show their key as a key cap: "[F] Take M870" reads F, then the text.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")
## White weapon silhouettes by weapon key; weapons without one show only the name.
const WEAPON_ICONS := "res://assets/ui/weapons/%s.png"

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
var _icon_key := ""
## The longest respawn wait seen this death, to fill the respawn bar.
var _respawn_total := 0.0
var _health_fill := UiTheme.plate(UiTheme.TEXT, 0.0, 0.0)
var _low_fill := UiTheme.plate(UiTheme.HARM, 0.0, 0.0)
var _mag_fill := UiTheme.plate(UiTheme.TEXT, 0.0, 0.0)
var _mag_low_fill := UiTheme.plate(UiTheme.ENEMY, 0.0, 0.0)

## Hit marker sounds by name, built on first use.
var _markers := {}

@onready var _player: FpsPlayer = get_node_or_null(player_path) as FpsPlayer
@onready var _crosshair: Control = $Crosshair
@onready var _readout: Label = $Readout
@onready var _controls: Label = $Controls
@onready var _weapon: Label = $AmmoBlock/Ammo/Top/Weapon
@onready var _weapon_icon: TextureRect = $AmmoBlock/Ammo/Top/WeaponIcon
@onready var _magazine: Label = $AmmoBlock/Ammo/Count/Magazine
@onready var _reserve: Label = $AmmoBlock/Ammo/Count/Reserve
@onready var _mag_bar: ProgressBar = $AmmoBlock/Ammo/MagBar
@onready var _warning: Control = $Warning
@onready var _prompt: Control = $Prompt
@onready var _health: ProgressBar = $HealthBlock/Row/Column/Health
@onready var _health_value: Label = $HealthBlock/Row/Column/HealthValue
@onready var _health_block: Control = $HealthBlock
@onready var _death: Control = $Death
@onready var _death_detail: Label = $Death/Detail
@onready var _respawn_bar: ProgressBar = $Death/RespawnBar
@onready var _damage: Control = $Damage
@onready var _scope: Control = $Scope
@onready var _hit_sound: AudioStreamPlayer = $HitSound
## A second voice, for the headshot ring under a headshot kill.
@onready var _head_sound: AudioStreamPlayer = _make_head_sound()
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


func _make_head_sound() -> AudioStreamPlayer:
	var player := AudioStreamPlayer.new()
	player.bus = &"UI"
	player.volume_db = _hit_sound.volume_db - 2.0
	add_child(player)
	return player


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("toggle_controls"):
		_controls_forced = not _controls.visible
		_controls_left = 0.0


func _on_hit(killed: bool, headshot: bool) -> void:
	_crosshair.show_hit(killed, headshot)
	# A dry tick for a hit, a struck-helmet ring for a headshot, a thump and
	# a short ding for a kill; a headshot kill plays both, so neither is lost.
	_hit_sound.stream = _marker("kill" if killed else ("hit_head" if headshot else "hit"))
	_hit_sound.play()
	if killed and headshot:
		_head_sound.stream = _marker("hit_head")
		_head_sound.play()


## The takes of a hit marker sound (`res://assets/audio/ui/<marker>_N.wav`),
## picked at random each time without the same take twice in a row.
func _marker(marker: String) -> AudioStream:
	if not _markers.has(marker):
		var randomizer := AudioStreamRandomizer.new()
		randomizer.random_pitch = 1.03
		for i in range(1, 9):
			var path := "res://assets/audio/ui/%s_%d.wav" % [marker, i]
			if not ResourceLoader.exists(path):
				break
			randomizer.add_stream(-1, load(path))
		_markers[marker] = randomizer
	return _markers[marker]


func _process(delta: float) -> void:
	if _player == null:
		return
	var show_fps: bool = Settings.show_fps
	_readout.visible = show_fps
	_readout.text = (
		"%s   %.1f M/S   %d FPS"
		% [
			_player.movement_state().to_upper(),
			_player.horizontal_speed(),
			Engine.get_frames_per_second(),
		]
	)
	_controls_left = maxf(_controls_left - delta, 0.0)
	_controls.visible = _controls_forced or _controls_left > 0.0
	_controls.text = GAMEPAD_CONTROLS if _player.using_gamepad() else _keyboard_controls

	var alive := _player.is_alive()
	var scoped := _player.scope_amount() >= 0.85
	_update_ammo()
	var magazine := _player.magazine()
	var reserve := _player.reserve()
	var low := magazine <= _player.low_ammo_threshold()

	_warning.visible = alive and not scoped
	if _player.is_reloading():
		_show_prompt(_warning, "RELOADING", UiTheme.TEXT_DIM)
	elif magazine == 0 and reserve == 0:
		_show_prompt(_warning, "NO AMMO", UiTheme.HARM)
	elif low and reserve > 0:
		_show_prompt(_warning, "[%s] RELOAD" % Settings.key_label(&"reload"), UiTheme.ACCENT)
	elif low:
		_show_prompt(_warning, "LOW AMMO", UiTheme.ENEMY)
	else:
		_warning.visible = false

	var prompt := _player.interact_prompt()
	_prompt.visible = alive and prompt != ""
	if _prompt.visible:
		_show_prompt(_prompt, prompt, UiTheme.TEXT)

	_update_health()

	# Without respawns (Zombies) the mode shows its own end screen.
	_death.visible = not alive and _player.auto_respawn
	if _death.visible:
		var left := _player.respawn_time_left()
		_respawn_total = maxf(_respawn_total, left)
		_death_detail.text = "Respawning in %.1f" % left
		_respawn_bar.max_value = maxf(_respawn_total, 0.01)
		_respawn_bar.value = _respawn_total - left
	else:
		_respawn_total = 0.0

	# Convert the spread cone to pixels so the crosshair shows where shots can land.
	# Shotguns add their pellet cone, so the crosshair matches the pattern.
	var half_fov := deg_to_rad(_player.camera_fov()) * 0.5
	var pixels_per_unit := get_viewport().get_visible_rect().size.y * 0.5 / tan(half_fov)
	var spread := _player.spread_radians()
	if _player.pellet_count() > 1:
		spread += _player.pellet_spread_radians()
	_crosshair.gap = tan(spread) * pixels_per_unit
	_crosshair.alpha = 0.0 if not alive else 1.0 - clampf(_player.ads_amount() * 1.5, 0.0, 1.0)


func _update_ammo() -> void:
	_weapon.text = _player.weapon_name().to_upper()
	var key := _player.weapon_key()
	if key != _icon_key:
		_icon_key = key
		var path := WEAPON_ICONS % key
		_weapon_icon.texture = load(path) as Texture2D if ResourceLoader.exists(path) else null
		_weapon_icon.visible = _weapon_icon.texture != null
	var magazine := _player.magazine()
	var low := magazine <= _player.low_ammo_threshold()
	_magazine.text = str(magazine)
	_magazine.add_theme_color_override(&"font_color", UiTheme.ENEMY if low else UiTheme.TEXT)
	_reserve.text = "/%d" % _player.reserve()
	_mag_bar.max_value = maxi(_player.magazine_size(), 1)
	_mag_bar.value = magazine
	_mag_bar.add_theme_stylebox_override(&"fill", _mag_low_fill if low else _mag_fill)


func _update_health() -> void:
	var health := _player.health()
	var full := _player.max_health()
	_health.max_value = full
	_health.value = health
	_health_value.text = str(ceili(health))
	var hurt := health < full * 0.35
	_health.add_theme_stylebox_override(&"fill", _low_fill if hurt else _health_fill)
	_health_value.add_theme_color_override(&"font_color", UiTheme.HARM if hurt else UiTheme.TEXT)
	# Quiet at full health; fully up once hurt.
	_health_block.modulate.a = 1.0 if health < full else 0.55


## Shows `text` in a prompt row. A leading "[KEY]" becomes the row's key cap.
func _show_prompt(row: Control, text: String, colour: Color) -> void:
	var key := ""
	if text.begins_with("["):
		var close := text.find("]")
		if close > 0:
			key = text.substr(1, close - 1)
			text = text.substr(close + 1).strip_edges()
	var cap := row.get_node("Key") as Label
	cap.text = key
	cap.visible = key != ""
	var label := row.get_node("Text") as Label
	label.text = text
	label.add_theme_color_override(&"font_color", colour)
