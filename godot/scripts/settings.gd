extends Node
## Player settings (autoload `Settings`). Loads and saves `user://settings.cfg`,
## applies the engine-level settings itself (window mode, vsync, frame cap, bus
## volumes), and pushes the gameplay ones onto the player with `apply_to_player`.
##
## Read values as properties (`Settings.fov`). Change them through `set_setting`,
## which validates the value, applies it, emits `changed`, and saves shortly after.

## Emitted after any setting changes, including on load and reset.
signal changed

## Radians of view rotation per screen pixel at a sensitivity of 1.0.
const BASE_MOUSE_SENSITIVITY := 0.0022
## Frame caps offered in the menu. 0 means unlimited.
const MAX_FPS_OPTIONS: Array[int] = [0, 60, 120, 144, 165, 240]
## Seconds to wait after the last change before writing the file.
const SAVE_DELAY := 0.75

const DEFAULTS := {
	&"mouse_sensitivity": 1.0,
	&"ads_sensitivity": 1.0,
	&"fov": 102.0,
	&"camera_motion": true,
	&"fullscreen": false,
	&"vsync": true,
	&"max_fps": 0,
	&"show_fps": true,
	&"master_volume": 1.0,
	&"weapons_volume": 0.8,
	&"effects_volume": 0.8,
	&"ui_volume": 0.7,
	&"master_server": "50.67.76.237:27010",
}

## Config file section for each setting.
const SECTIONS := {
	"controls": [&"mouse_sensitivity", &"ads_sensitivity"],
	"video": [&"fov", &"camera_motion", &"fullscreen", &"vsync", &"max_fps", &"show_fps"],
	"audio": [&"master_volume", &"weapons_volume", &"effects_volume", &"ui_volume"],
	"multiplayer": [&"master_server"],
}

## Allowed range of each float setting.
const RANGES := {
	&"mouse_sensitivity": Vector2(0.1, 5.0),
	&"ads_sensitivity": Vector2(0.3, 2.0),
	&"fov": Vector2(80.0, 120.0),
	&"master_volume": Vector2(0.0, 1.0),
	&"weapons_volume": Vector2(0.0, 1.0),
	&"effects_volume": Vector2(0.0, 1.0),
	&"ui_volume": Vector2(0.0, 1.0),
}

## Audio bus driven by each volume setting.
const BUSES := {
	&"master_volume": &"Master",
	&"weapons_volume": &"Weapons",
	&"effects_volume": &"Effects",
	&"ui_volume": &"UI",
}

## File the settings load from and save to. Tests point this elsewhere.
var path := "user://settings.cfg"

## Multiplier on BASE_MOUSE_SENSITIVITY.
var mouse_sensitivity: float = DEFAULTS[&"mouse_sensitivity"]
## Sensitivity multiplier while aiming down sights.
var ads_sensitivity: float = DEFAULTS[&"ads_sensitivity"]
## Horizontal field of view in degrees at 16:9. See `vertical_fov`.
var fov: float = DEFAULTS[&"fov"]
## Head bob and strafe tilt.
var camera_motion: bool = DEFAULTS[&"camera_motion"]
var fullscreen: bool = DEFAULTS[&"fullscreen"]
var vsync: bool = DEFAULTS[&"vsync"]
## Frame cap; 0 is unlimited. Always one of MAX_FPS_OPTIONS.
var max_fps: int = DEFAULTS[&"max_fps"]
## Read by the HUD.
var show_fps: bool = DEFAULTS[&"show_fps"]
## Linear bus volumes, 0 to 1.
var master_volume: float = DEFAULTS[&"master_volume"]
var weapons_volume: float = DEFAULTS[&"weapons_volume"]
var effects_volume: float = DEFAULTS[&"effects_volume"]
var ui_volume: float = DEFAULTS[&"ui_volume"]
## Master server (`host[:port]`) the server browser lists internet servers
## from. Blank values from older installs use the public directory.
var master_server: String = DEFAULTS[&"master_server"]

var _dirty := false
var _save_timer := Timer.new()


func _init() -> void:
	_save_timer.one_shot = true
	_save_timer.wait_time = SAVE_DELAY
	_save_timer.process_mode = Node.PROCESS_MODE_ALWAYS
	_save_timer.timeout.connect(flush)
	add_child(_save_timer)


func _ready() -> void:
	load_settings()


func _exit_tree() -> void:
	flush()


## Converts the horizontal FOV at 16:9 to the vertical FOV the camera uses.
func vertical_fov() -> float:
	var half := deg_to_rad(fov) * 0.5
	return rad_to_deg(2.0 * atan(tan(half) * 9.0 / 16.0))


## Pushes the gameplay settings onto an FpsPlayer. Uses `set` so a property the
## player does not have yet is skipped instead of failing.
func apply_to_player(player: Node) -> void:
	if player == null or not is_instance_valid(player):
		return
	player.set(&"mouse_sensitivity", BASE_MOUSE_SENSITIVITY * mouse_sensitivity)
	player.set(&"ads_sensitivity_scale", ads_sensitivity)
	player.set(&"base_fov", vertical_fov())
	player.set(&"camera_motion", camera_motion)


## Validates, stores, and applies one setting, then schedules a save.
func set_setting(key: StringName, value: Variant) -> void:
	if not DEFAULTS.has(key):
		push_error("Unknown setting '%s'." % key)
		return
	var clean: Variant = _sanitize(key, value)
	if clean == get(key):
		return
	set(key, clean)
	_apply(key)
	_mark_dirty()
	changed.emit()


func reset_to_defaults() -> void:
	for key: StringName in DEFAULTS:
		set(key, DEFAULTS[key])
	apply_all()
	_mark_dirty()
	changed.emit()


## Reads `path`. Missing or invalid values fall back to their defaults.
func load_settings() -> void:
	var config := ConfigFile.new()
	var err := config.load(path)
	if err != OK and err != ERR_FILE_NOT_FOUND:
		push_warning("Could not read %s (%s); using defaults." % [path, error_string(err)])
	for section: String in SECTIONS:
		for key: StringName in SECTIONS[section]:
			var stored: Variant = (
				config.get_value(section, key, DEFAULTS[key]) if err == OK else DEFAULTS[key]
			)
			set(key, _sanitize(key, stored))
	_dirty = false
	_save_timer.stop()
	apply_all()
	changed.emit()


func save_settings() -> Error:
	var config := ConfigFile.new()
	for section: String in SECTIONS:
		for key: StringName in SECTIONS[section]:
			config.set_value(section, key, get(key))
	var err := config.save(path)
	if err != OK:
		push_warning("Could not save %s (%s)." % [path, error_string(err)])
		return err
	_dirty = false
	_save_timer.stop()
	return OK


## Saves now if anything changed since the last save.
func flush() -> void:
	if _dirty:
		save_settings()


func apply_all() -> void:
	for key: StringName in DEFAULTS:
		_apply(key)


func _apply(key: StringName) -> void:
	if BUSES.has(key):
		var bus := AudioServer.get_bus_index(BUSES[key])
		if bus >= 0:
			var volume: float = get(key)
			AudioServer.set_bus_volume_db(bus, linear_to_db(volume))
			AudioServer.set_bus_mute(bus, volume <= 0.0)
		return
	match key:
		&"max_fps":
			Engine.max_fps = max_fps
		&"fullscreen":
			_apply_window_mode()
		&"vsync":
			# Mailbox vsync doesn't tear and queues no frames, so it adds less input
			# lag than standard vsync. Godot falls back to standard vsync where the
			# driver lacks mailbox. Matches `display/window/vsync/vsync_mode`.
			if not _headless():
				DisplayServer.window_set_vsync_mode(
					DisplayServer.VSYNC_MAILBOX if vsync else DisplayServer.VSYNC_DISABLED
				)


func _apply_window_mode() -> void:
	if _headless():
		return
	var mode := DisplayServer.window_get_mode()
	var is_fullscreen := (
		mode == DisplayServer.WINDOW_MODE_FULLSCREEN
		or mode == DisplayServer.WINDOW_MODE_EXCLUSIVE_FULLSCREEN
	)
	if fullscreen and not is_fullscreen:
		# Exclusive fullscreen gives the lowest latency on Windows; elsewhere it
		# behaves like borderless fullscreen.
		DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_EXCLUSIVE_FULLSCREEN)
	elif not fullscreen and is_fullscreen:
		DisplayServer.window_set_mode(DisplayServer.WINDOW_MODE_WINDOWED)


## Coerces a value from the menu or the config file to the setting's type and
## range, or returns the default when it cannot.
func _sanitize(key: StringName, value: Variant) -> Variant:
	var default: Variant = DEFAULTS[key]
	var number := value is int or value is float
	match typeof(default):
		TYPE_BOOL:
			return value if value is bool else default
		TYPE_INT:
			return int(value) if number and int(value) in MAX_FPS_OPTIONS else default
		TYPE_FLOAT:
			if not number or is_nan(float(value)):
				return default
			var limits: Vector2 = RANGES[key]
			return clampf(float(value), limits.x, limits.y)
		TYPE_STRING:
			if value is String or value is StringName:
				var text := String(value).strip_edges().left(255)
				return text if not text.is_empty() else default
	return default


func _mark_dirty() -> void:
	_dirty = true
	if _save_timer.is_inside_tree():
		_save_timer.start()


func _headless() -> bool:
	return DisplayServer.get_name() == "headless"
