extends CanvasLayer
## Heads-up display: crosshair, hit markers and sounds, score popups and
## medals, the weapon and ammo cluster with low-ammo warnings, health, damage
## feedback, the scope overlay, pickup prompts, the death card, the controls
## legend, and the frame-rate counter.
##
## Layout: health bottom left (`hud_vitals.gd`), the weapon cluster bottom
## right (`hud_weapon.gd`), prompts under the crosshair and the death card in
## the lower third (`hud_centre.gd`), your score top left (`score_feed.gd`),
## and the frame rate top right. Match HUDs add their own pieces on top and
## tell this one who killed us (`set_killer`).

const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const Paint := preload("res://scripts/ui/hud_paint.gd")

## Seconds the controls legend stays up after the first spawn; F1 toggles it.
const CONTROLS_TIME := 14.0

## The controls legend by default: "action:Label" pairs. `move` shows the four
## movement keys; `weapon_1` shows both weapon keys.
const DEFAULT_CONTROLS := [
	"move:Move",
	"sprint:Sprint",
	"crouch:Crouch / slide",
	"prone:Prone",
	"jump:Jump",
	"fire:Fire",
	"aim:Aim",
	"reload:Reload",
	"melee:Knife",
	"lethal:Frag",
	"tactical:Tactical",
	"weapon_1:Switch weapon",
	"interact:Interact",
	"refill_ammo:Refill ammo",
	"pause:Menu",
	"toggle_controls:Hide controls",
]

## Seconds the death card shows in a mode without respawns, before its
## spectating takes over (`net/snd_hud.gd` waits 1.5 s).
const SND_CARD_SECONDS := 1.45

@export var player_path: NodePath
## Replaces the controls legend ("action:Label" pairs), for levels whose
## controls differ.
@export var controls: PackedStringArray = []

var _controls_left := CONTROLS_TIME
## Seconds since we died (0 while alive).
var _dead_time := 0.0
var _controls_forced := false
## The longest respawn wait seen this death, to fill the respawn bar.
var _respawn_total := 0.0
## The layer the crosshair draws on, over objective markers.
var _top: CanvasLayer

## Hit marker sounds by name, built on first use.
var _markers := {}

@onready var _player: FpsPlayer = get_node_or_null(player_path) as FpsPlayer
@onready var _crosshair: Control = $Crosshair
@onready var _readout: Label = $Readout
@onready var _centre: Control = $Centre
@onready var _health: Control = $HealthBlock
@onready var _weapon: Control = $AmmoBlock
@onready var _damage: Control = $Damage
@onready var _scope: Control = $Scope
@onready var _hit_sound: AudioStreamPlayer = $HitSound
## A second voice, for the headshot ring under a headshot kill.
@onready var _head_sound: AudioStreamPlayer = _make_head_sound()
@onready var _score_feed: Control = $ScoreFeed


func _ready() -> void:
	if _player == null:
		return
	_player.hit_confirmed.connect(_on_hit)
	_player.scored.connect(_score_feed.add_award)
	_score_feed.player = _player
	_player.damaged.connect(_damage.on_damaged)
	# Whether the last hurt was a fall, for the death card when no one killed us.
	_player.damaged.connect(
		func(_amount: float, _from: Vector3, fall: bool) -> void: _centre.fell = fall
	)
	_player.respawned.connect(_on_respawned)
	_damage.player = _player
	_scope.player = _player
	_health.player = _player
	_weapon.player = _player
	_add_grenade_danger()
	Settings.fit_hud(_health, true)
	Settings.fit_hud(_weapon)
	Settings.fit_hud(_centre)
	Settings.fit_hud(_score_feed)
	# The health ring draws after the weapon cluster, so the cluster's corner
	# shade does not dim its number.
	move_child(_health, _weapon.get_index())
	# The crosshair and hit markers sit above everything else drawn over the
	# world, objective markers included (they are on layer 4). The damage
	# overlay stays under the HUD, so its tint never dims the numbers.
	_top = CanvasLayer.new()
	_top.name = "HudTop"
	_top.layer = 5
	add_child(_top)
	_crosshair.reparent(_top)


## The grenade danger indicator round the crosshair (`ui/grenade_danger.gd`).
func _add_grenade_danger() -> void:
	var danger: Control = (load("res://scripts/ui/grenade_danger.gd") as GDScript).new()
	danger.name = "GrenadeDanger"
	danger.set_anchors_preset(Control.PRESET_FULL_RECT)
	danger.mouse_filter = Control.MOUSE_FILTER_IGNORE
	danger.set("player", _player)
	add_child(danger)
	move_child(danger, _crosshair.get_index() + 1)


func _make_head_sound() -> AudioStreamPlayer:
	var player := AudioStreamPlayer.new()
	player.bus = &"UI"
	player.volume_db = _hit_sound.volume_db - 2.0
	add_child(player)
	return player


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("toggle_controls"):
		_controls_forced = not _legend_showing()
		_controls_left = 0.0


## Names who killed us on the death card, with their colour, weapon key, and
## the kill feed's name for what did it ("Knife", "Frag", …); `self_kill`
## when we killed ourselves (match HUDs call this when the kill is reported).
func set_killer(
	killer_name: String, colour: Color, weapon_key := "", cause := "", self_kill := false
) -> void:
	_centre.killer = killer_name
	_centre.killer_colour = colour
	_centre.killer_weapon = weapon_key
	_centre.killer_cause = cause
	_centre.killer_self = self_kill


## Names the player we just killed on the "Killed" plate (match HUDs call
## this when they see our kill).
func show_victim(victim_name: String, colour: Color) -> void:
	_centre.show_victim(victim_name, colour)


func _on_respawned() -> void:
	_controls_left = 0.0
	set_killer("", TeamColours.enemy)
	_centre.fell = false


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


func _legend_showing() -> bool:
	return _controls_forced or _controls_left > 0.0


func _process(delta: float) -> void:
	if _player == null:
		return
	_readout.visible = Settings.show_fps
	if _readout.visible:
		_readout.text = "%d FPS" % Engine.get_frames_per_second()

	_controls_left = maxf(_controls_left - delta, 0.0)
	var legend_target := 1.0 if _legend_showing() else 0.0
	_centre.legend_alpha = move_toward(_centre.legend_alpha, legend_target, delta * 3.0)
	if _centre.legend_alpha > 0.0:
		_centre.legend = _legend(_player.using_gamepad())

	# A killcam draws its own overlay: the HUD steps aside (the killer's
	# scope still shows, from `scope_amount`).
	var killcam := _player.in_killcam()
	# The results screen clears the play HUD, as in Call of Duty; the final
	# killcam before it still needs the scope.
	visible = killcam or not _match_ended()
	_top.visible = visible
	for piece: Control in [_crosshair, _centre, _health, _weapon, _damage, _score_feed]:
		piece.visible = not killcam
	if killcam:
		return
	_crosshair.visible = true
	_centre.visible = true
	_damage.visible = true
	_score_feed.visible = true
	var alive := _player.is_alive()
	var scoped := _player.scope_amount() >= 0.85
	var magazine := _player.magazine()
	var reserve := _player.reserve()
	var low := magazine <= _player.low_ammo_threshold()

	var warning := {}
	if alive and not scoped:
		if _player.is_reloading():
			warning = {"text": "RELOADING", "colour": UiTheme.TEXT_DIM, "steady": true}
		elif magazine == 0 and reserve == 0:
			warning = {"text": "NO AMMO", "colour": UiTheme.HARM}
		elif low and reserve > 0:
			warning = Paint.split_key("[%s] RELOAD" % Settings.prompt_key(&"reload"), UiTheme.TEXT)
		elif low:
			warning = {"text": "LOW AMMO", "colour": UiTheme.READY}
	_centre.warning = warning

	var prompt := _player.interact_prompt()
	_centre.prompt = Paint.split_key(prompt, UiTheme.TEXT) if alive and prompt != "" else {}

	# Without respawns (Search and Destroy) the card shows until the mode's
	# own spectating takes over.
	_dead_time = 0.0 if alive else _dead_time + delta
	var dead := not alive and (_player.auto_respawn or _dead_time < SND_CARD_SECONDS)
	# The held scoreboard covers the middle: the death card waits under it.
	_centre.dead = dead and not Input.is_action_pressed(&"scoreboard")
	if dead:
		var left := _player.respawn_time_left()
		_respawn_total = maxf(_respawn_total, left)
		_centre.respawn_left = left
		_centre.respawn_total = _respawn_total
	else:
		_respawn_total = 0.0
	# Hardcore has no HUD: no crosshair, health, or ammo (Black Ops 2's
	# `hud_hardcore`). Hit markers, prompts, and the death card stay.
	var hardcore := _hardcore()
	_health.visible = alive and not hardcore
	_weapon.visible = alive and not hardcore

	# Convert the spread cone to pixels so the crosshair shows where shots can land.
	# Shotguns add their pellet cone, so the crosshair matches the pattern.
	var half_fov := deg_to_rad(_player.camera_fov()) * 0.5
	var pixels_per_unit := get_viewport().get_visible_rect().size.y * 0.5 / tan(half_fov)
	var spread := _player.spread_radians()
	if _player.pellet_count() > 1:
		spread += _player.pellet_spread_radians()
	_crosshair.gap = tan(spread) * pixels_per_unit
	_crosshair.alpha = (
		0.0 if not alive or hardcore else 1.0 - clampf(_player.ads_amount() * 1.5, 0.0, 1.0)
	)
	_crosshair.enemy = _aiming_at_enemy()


## True online while the crosshair is on an enemy (found at run time,
## untyped, as `_match_ended` is).
func _aiming_at_enemy() -> bool:
	var net := get_node_or_null(^"/root/Net")
	var client: Object = net.call(&"client") if net != null else null
	return client != null and bool(client.call(&"aiming_at_enemy"))


## True in an online custom game with hardcore's bare HUD (found at run
## time, untyped, as `_match_ended` is).
func _hardcore() -> bool:
	var net := get_node_or_null(^"/root/Net")
	var client: Object = net.call(&"client") if net != null else null
	return (
		client != null and bool((client.call(&"match_info") as Dictionary).get("hardcore", false))
	)


## True on an online match's results screen.
func _match_ended() -> bool:
	# Found at run time, untyped: naming the Net autoload and NetClient here
	# changed the offline arena's frame-by-frame results (smoke_test's view
	# drift checks), though nothing in it runs offline.
	var net := get_node_or_null(^"/root/Net")
	var client: Object = net.call(&"client") if net != null else null
	if client == null:
		return false
	var info: Dictionary = client.call(&"match_info")
	return String(info.get("phase", "")) == "ended"


## The legend's [key, label] pairs for the current device.
func _legend(gamepad: bool) -> Array:
	var pairs := []
	for entry in controls if not controls.is_empty() else PackedStringArray(DEFAULT_CONTROLS):
		var action := entry.get_slice(":", 0)
		var label := entry.get_slice(":", 1)
		# The ammo refill is the Testing Area's alone (off online).
		if action == "refill_ammo" and not _player.get(&"allow_refill"):
			continue
		var key := ""
		match action:
			"move":
				key = (
					"pad:stick_l"
					if gamepad
					else _keys([&"move_forward", &"move_left", &"move_back", &"move_right"], "")
				)
			"weapon_1":
				key = (
					Settings.key_label(&"weapon_next", true)
					if gamepad
					else _keys([&"weapon_1", &"weapon_2"], "/")
				)
			_:
				key = Settings.key_label(StringName(action), gamepad)
		if key == "?":
			continue
		pairs.append([key, label])
	if gamepad:
		pairs.insert(1, ["pad:stick_r", "Look"])
	return pairs


func _keys(actions: Array, separator: String) -> String:
	var keys := PackedStringArray()
	for action: StringName in actions:
		keys.append(Settings.key_label(action))
	return separator.join(keys)
