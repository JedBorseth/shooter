extends Node
## Player settings (autoload `Settings`). Loads and saves `user://settings.cfg`,
## applies the engine-level settings itself (window mode, vsync, frame cap, bus
## volumes, the input map), and pushes the gameplay ones onto the player with
## `apply_to_player`.
##
## The input device (`input_device`) picks keyboard and mouse or a controller
## for play. The other device's bindings are taken off every gameplay action,
## so it cannot move, aim, or fire; menus (`ui_*`) and SHARED_ACTIONS take both.
##
## Read values as properties (`Settings.fov`). Change them through `set_setting`,
## which validates the value, applies it, emits `changed`, and saves shortly after.

## Emitted after any setting changes, including on load and reset.
signal changed

## What plays the game, by index into INPUT_DEVICES.
enum InputDevice { KEYBOARD_MOUSE, CONTROLLER }

const GraphicsQuality := preload("res://scripts/graphics.gd")
const Lens := preload("res://scripts/lens.gd")
const ViewmodelLight := preload("res://scripts/viewmodel_light.gd")
const EyeAdaptation := preload("res://scripts/eye_adaptation.gd")
const Grounding := preload("res://scripts/grounding.gd")
const InputNotice := preload("res://scripts/ui/input_notice.gd")

## Radians of view rotation per screen pixel at 1× sensitivity.
const BASE_MOUSE_SENSITIVITY := 0.0022
## Controller look speed is a level from 1 to SENSITIVITY_LEVELS, as in Call of
## Duty (Black Ops Cold War uses the same 1 to 14). Each level turns
## SENSITIVITY_STEP times faster than the one below, so every step feels like
## the same change; SENSITIVITY_UNIT_LEVEL turns at the base stick rate (1×).
## The default, level 7, is one step above it. Mouse sensitivity stays a
## multiplier.
const SENSITIVITY_LEVELS := 14
const SENSITIVITY_STEP := 1.2
const SENSITIVITY_UNIT_LEVEL := 6
const SENSITIVITY_LEVEL_OPTIONS: Array[int] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
## The band each look speed level falls in, from level 1 up. Like Call of
## Duty's menus, every level shows its band.
const SENSITIVITY_NAMES: Array[String] = [
	"VERY LOW",
	"VERY LOW",
	"LOW",
	"LOW",
	"LOW",
	"MEDIUM",
	"MEDIUM",
	"MEDIUM",
	"HIGH",
	"HIGH",
	"HIGH",
	"HIGH",
	"VERY HIGH",
	"VERY HIGH",
]
## Frame caps offered in the menu. 0 means unlimited.
const MAX_FPS_OPTIONS: Array[int] = [0, 60, 120, 144, 165, 240]
## Seconds to wait after the last change before writing the file.
const SAVE_DELAY := 0.75

## Audio mixes offered in the menu, by index. Each sets how wide positional
## sounds pan (a multiplier on Godot's base 3D panning strength, which is tuned
## for headphones: 0 centred, 1 as tuned), bus offsets in dB on top of the volume
## sliders, and whether the Master bus compresses (quiet sounds lifted, loud
## ones held down). Competitive brings footsteps forward and the ambience down.
const AUDIO_MIXES: Array[String] = ["Headphones", "Speakers", "Competitive"]
const MIX_PANNING: Array[float] = [1.0, 0.6, 1.5]
const MIX_BUS_OFFSETS: Array[Dictionary] = [
	{},
	{},
	{&"Footsteps": 4.0, &"Ambience": -9.0, &"Weapons": -3.0},
]
const MIX_COMPRESSION: Array[bool] = [false, false, true]

## Shared public host for discovery and future account/matchmaking services.
const SERVICE_HOST := "borseth.ddns.net"
const DEFAULT_DIRECTORY := SERVICE_HOST + ":27010"
## Base stick turn rates (degrees per second) at 1× look speed.
const STICK_YAW_SPEED := 200.0
const STICK_PITCH_SPEED := 150.0

const INPUT_DEVICES: Array[String] = ["Keyboard & mouse", "Controller"]
## Actions that keep both devices' bindings whichever is chosen: a way back to
## the menu from either, and typing, which needs the keyboard. `ui_*` actions
## (menu navigation) also keep both.
const SHARED_ACTIONS: Array[StringName] = [
	&"pause", &"chat", &"team_chat", &"toggle_controls", &"debug_hitboxes"
]

## Actions the player can rebind, with their names in the menu. Only keyboard
## and mouse bindings change; gamepad bindings stay as they are.
const REBINDABLE := {
	&"move_forward": "Move forward",
	&"move_back": "Move back",
	&"move_left": "Move left",
	&"move_right": "Move right",
	&"jump": "Jump",
	&"sprint": "Sprint",
	&"crouch": "Crouch / slide (hold: prone)",
	&"prone": "Prone",
	&"fire": "Fire",
	&"aim": "Aim",
	&"reload": "Reload",
	&"melee": "Melee",
	&"lethal": "Lethal (frag grenade)",
	&"tactical": "Tactical (flashbang or smoke)",
	&"weapon_1": "Primary weapon",
	&"weapon_2": "Secondary weapon",
	&"streak_use": "Use scorestreak",
	&"streak_next": "Next scorestreak",
	&"interact": "Interact",
	&"inspect": "Inspect weapon",
	&"loadout": "Loadout",
	&"scoreboard": "Scoreboard",
	&"chat": "Chat",
	&"team_chat": "Team chat",
}

## Xbox-style names of gamepad buttons, by `JoyButton`.
const JOY_BUTTON_NAMES := {
	JOY_BUTTON_A: "A",
	JOY_BUTTON_B: "B",
	JOY_BUTTON_X: "X",
	JOY_BUTTON_Y: "Y",
	JOY_BUTTON_BACK: "View",
	JOY_BUTTON_GUIDE: "Guide",
	JOY_BUTTON_START: "Menu",
	JOY_BUTTON_LEFT_STICK: "LS",
	JOY_BUTTON_RIGHT_STICK: "RS",
	JOY_BUTTON_LEFT_SHOULDER: "LB",
	JOY_BUTTON_RIGHT_SHOULDER: "RB",
	JOY_BUTTON_DPAD_UP: "D-pad up",
	JOY_BUTTON_DPAD_DOWN: "D-pad down",
	JOY_BUTTON_DPAD_LEFT: "D-pad left",
	JOY_BUTTON_DPAD_RIGHT: "D-pad right",
}

const MOUSE_BUTTON_NAMES := {
	MOUSE_BUTTON_LEFT: "LMB",
	MOUSE_BUTTON_RIGHT: "RMB",
	MOUSE_BUTTON_MIDDLE: "MMB",
	MOUSE_BUTTON_WHEEL_UP: "Wheel up",
	MOUSE_BUTTON_WHEEL_DOWN: "Wheel down",
	MOUSE_BUTTON_XBUTTON1: "Mouse 4",
	MOUSE_BUTTON_XBUTTON2: "Mouse 5",
}

const DEFAULTS := {
	&"input_device": InputDevice.KEYBOARD_MOUSE,
	&"mouse_sensitivity": 1.0,
	&"ads_sensitivity": 1.0,
	&"invert_mouse_y": false,
	&"invert_stick_y": false,
	&"stick_sensitivity_level": 7,
	&"stick_deadzone": 0.1,
	&"stick_curve": 2.0,
	&"aim_assist": true,
	&"rumble": true,
	&"fov": 102.0,
	&"camera_motion": true,
	&"fullscreen": false,
	&"vsync": true,
	&"max_fps": 0,
	&"show_fps": false,
	&"graphics_quality": GraphicsQuality.Preset.MEDIUM,
	&"render_scale": 1.0,
	&"motion_blur": false,
	&"lens_effects": true,
	&"master_volume": 1.0,
	&"weapons_volume": 0.8,
	&"effects_volume": 0.8,
	&"footsteps_volume": 0.9,
	&"ambience_volume": 0.6,
	&"ui_volume": 0.7,
	&"audio_mix": 0,
	&"master_server": DEFAULT_DIRECTORY,
	&"player_name": "Player",
	&"skip_killcams": false,
	&"minimap_rotates": true,
}

## Config file section for each setting.
const SECTIONS := {
	"controls": [&"input_device", &"mouse_sensitivity", &"ads_sensitivity", &"invert_mouse_y"],
	"gamepad":
	[
		&"invert_stick_y",
		&"stick_sensitivity_level",
		&"stick_deadzone",
		&"stick_curve",
		&"aim_assist",
		&"rumble",
	],
	"video":
	[
		&"fov",
		&"camera_motion",
		&"fullscreen",
		&"vsync",
		&"max_fps",
		&"show_fps",
		&"graphics_quality",
		&"render_scale",
		&"motion_blur",
		&"lens_effects",
	],
	"audio":
	[
		&"master_volume",
		&"weapons_volume",
		&"effects_volume",
		&"footsteps_volume",
		&"ambience_volume",
		&"ui_volume",
		&"audio_mix",
	],
	"multiplayer": [&"player_name", &"skip_killcams", &"minimap_rotates"],
}

## Allowed values of each int setting.
const INT_OPTIONS := {
	&"input_device": [InputDevice.KEYBOARD_MOUSE, InputDevice.CONTROLLER],
	&"stick_sensitivity_level": SENSITIVITY_LEVEL_OPTIONS,
	&"max_fps": MAX_FPS_OPTIONS,
	&"graphics_quality": [0, 1, 2],
	&"audio_mix": [0, 1, 2],
}

## Look speed saved by builds before it became a level, as a multiplier.
const LEGACY_STICK_SENSITIVITY := &"stick_sensitivity"
## Mouse sensitivity saved by builds that briefly made it a level too.
const LEGACY_MOUSE_LEVEL := &"mouse_sensitivity_level"

## Allowed range of each float setting.
const RANGES := {
	&"mouse_sensitivity": Vector2(0.1, 5.0),
	&"ads_sensitivity": Vector2(0.3, 2.0),
	&"stick_deadzone": Vector2(0.02, 0.3),
	&"stick_curve": Vector2(1.0, 3.0),
	&"fov": Vector2(80.0, 120.0),
	&"master_volume": Vector2(0.0, 1.0),
	&"weapons_volume": Vector2(0.0, 1.0),
	&"effects_volume": Vector2(0.0, 1.0),
	&"footsteps_volume": Vector2(0.0, 1.0),
	&"ambience_volume": Vector2(0.0, 1.0),
	&"ui_volume": Vector2(0.0, 1.0),
	&"render_scale": Vector2(0.5, 1.0),
}

## Audio bus driven by each volume setting.
const BUSES := {
	&"master_volume": &"Master",
	&"weapons_volume": &"Weapons",
	&"effects_volume": &"Effects",
	&"footsteps_volume": &"Footsteps",
	&"ambience_volume": &"Ambience",
	&"ui_volume": &"UI",
}

## File the settings load from and save to. Tests point this elsewhere.
var path := "user://settings.cfg"

## An `InputDevice`: the one that plays. The other is locked out of play.
var input_device: int = DEFAULTS[&"input_device"]
## Multiplier on BASE_MOUSE_SENSITIVITY.
var mouse_sensitivity: float = DEFAULTS[&"mouse_sensitivity"]
## Sensitivity multiplier while aiming down sights.
var ads_sensitivity: float = DEFAULTS[&"ads_sensitivity"]
var invert_mouse_y: bool = DEFAULTS[&"invert_mouse_y"]
## Gamepad look: inversion, look speed level, inner deadzone, response curve
## exponent, aim assist, and vibration.
var invert_stick_y: bool = DEFAULTS[&"invert_stick_y"]
var stick_sensitivity_level: int = DEFAULTS[&"stick_sensitivity_level"]
var stick_deadzone: float = DEFAULTS[&"stick_deadzone"]
var stick_curve: float = DEFAULTS[&"stick_curve"]
var aim_assist: bool = DEFAULTS[&"aim_assist"]
var rumble: bool = DEFAULTS[&"rumble"]
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
## A `GraphicsQuality.Preset`.
var graphics_quality: int = DEFAULTS[&"graphics_quality"]
## Fraction of the window the 3D view renders at; FSR upscales below 1.0.
var render_scale: float = DEFAULTS[&"render_scale"]
## Camera and object motion blur (scripts/motion_blur.gd). Off by default.
var motion_blur: bool = DEFAULTS[&"motion_blur"]
## The vignette and film grain of the lens pass (scripts/lens.gd). Sharpening
## follows the graphics preset.
var lens_effects: bool = DEFAULTS[&"lens_effects"]
## Linear bus volumes, 0 to 1.
var master_volume: float = DEFAULTS[&"master_volume"]
var weapons_volume: float = DEFAULTS[&"weapons_volume"]
var effects_volume: float = DEFAULTS[&"effects_volume"]
var footsteps_volume: float = DEFAULTS[&"footsteps_volume"]
var ambience_volume: float = DEFAULTS[&"ambience_volume"]
var ui_volume: float = DEFAULTS[&"ui_volume"]
## An index into AUDIO_MIXES.
var audio_mix: int = DEFAULTS[&"audio_mix"]
## Master server (`host[:port]`) the server browser lists internet servers
## from. Runtime only: --directory HOST[:PORT] overrides the shared default.
## Legacy saved addresses are ignored so old installs follow the public hostname.
var master_server: String = DEFAULTS[&"master_server"]
## The name other players see (the server cuts it to 16 characters).
var player_name: String = DEFAULTS[&"player_name"]
## Respawn without watching the killcam after each death (Call of Duty's
## killcams are skipped one at a time; this skips them all). The killcam
## that ends a round or the match still plays.
var skip_killcams: bool = DEFAULTS[&"skip_killcams"]
## The minimap turns with the player, who always faces up (Call of Duty's
## default); off, north stays up and the player's arrow turns.
var minimap_rotates: bool = DEFAULTS[&"minimap_rotates"]

## Keyboard and mouse bindings changed from the defaults: action to a
## serialized event (see `_event_to_value`).
var key_bindings := {}

## Every binding of each locked gameplay action, both devices', before the
## unused device's were taken off the input map. Labels and rebinding read it.
var _all_events: Dictionary[StringName, Array] = {}

var _dirty := false
var _save_timer := Timer.new()
## The full-screen lens pass; none on a dedicated server.
var _lens: Lens


func _init() -> void:
	_save_timer.one_shot = true
	_save_timer.wait_time = SAVE_DELAY
	_save_timer.process_mode = Node.PROCESS_MODE_ALWAYS
	_save_timer.timeout.connect(flush)
	add_child(_save_timer)


func _ready() -> void:
	get_tree().node_added.connect(_on_node_added)
	if not _headless():
		_lens = Lens.new()
		add_child(_lens)
		add_child(ViewmodelLight.new())
		add_child(EyeAdaptation.new())
		var notice := InputNotice.new()
		notice.name = "InputNotice"
		notice.settings = self
		add_child(notice)
	load_settings()


## Client developer override, separate from --master (server registration).
## This is deliberately not saved: the next ordinary launch uses the public host.
static func directory_address(args: PackedStringArray) -> String:
	for i in args.size():
		if args[i] == "--directory" and i + 1 < args.size():
			var address := args[i + 1].strip_edges()
			if not address.is_empty() and not address.begins_with("--"):
				return address
	return DEFAULT_DIRECTORY


## Picks a graphics preset together with that preset's render scale, as the
## menu does. `set_setting(&"graphics_quality", ...)` alone keeps the scale.
func set_graphics_preset(preset: int) -> void:
	set_setting(&"graphics_quality", preset)
	set_setting(
		&"render_scale",
		GraphicsQuality.DEFAULT_RENDER_SCALE[clampi(preset, 0, GraphicsQuality.NAMES.size() - 1)]
	)


func _exit_tree() -> void:
	flush()


## How much faster than the base rate a sensitivity level turns.
static func sensitivity_scale(level: int) -> float:
	return pow(SENSITIVITY_STEP, level - SENSITIVITY_UNIT_LEVEL)


## The level closest to a turn-rate multiplier, for settings saved as one.
static func sensitivity_level(scale: float) -> int:
	if not scale > 0.0:
		return 1
	var level := SENSITIVITY_UNIT_LEVEL + log(scale) / log(SENSITIVITY_STEP)
	return clampi(roundi(level), 1, SENSITIVITY_LEVELS)


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
	player.set(&"mouse_invert_y", invert_mouse_y)
	player.set(&"stick_invert_y", invert_stick_y)
	var look_speed := sensitivity_scale(stick_sensitivity_level)
	player.set(&"stick_yaw_speed", STICK_YAW_SPEED * look_speed)
	player.set(&"stick_pitch_speed", STICK_PITCH_SPEED * look_speed)
	player.set(&"stick_deadzone", stick_deadzone)
	player.set(&"stick_curve", stick_curve)
	player.set(&"aim_assist", aim_assist)
	player.set(&"rumble", rumble)
	player.set(&"controller_input", uses_controller())


## The key or button bound to `action` on the device that plays, for prompts.
func prompt_key(action: StringName) -> String:
	return key_label(action, uses_controller())


## True when a controller plays and keyboard and mouse are locked out.
func uses_controller() -> bool:
	return input_device == InputDevice.CONTROLLER


## True if `event` comes from a controller (a button or a stick or trigger).
static func is_controller_event(event: InputEvent) -> bool:
	return event is InputEventJoypadButton or event is InputEventJoypadMotion


## Every binding of `action`, for both devices, whichever is locked out.
func action_events(action: StringName) -> Array:
	if _all_events.has(action):
		return _all_events[action]
	return InputMap.action_get_events(action) if InputMap.has_action(action) else []


## The key or button bound to `action`, for prompts ("F", "LMB", or with
## `gamepad`, "X"). On a gamepad, interact shares the reload button.
func key_label(action: StringName, gamepad := false) -> String:
	if gamepad and action == &"interact":
		action = &"reload"
	if not InputMap.has_action(action):
		return "?"
	for event: InputEvent in action_events(action):
		if gamepad and event is InputEventJoypadButton:
			var button := (event as InputEventJoypadButton).button_index
			return JOY_BUTTON_NAMES.get(button, "Button %d" % button)
		if not gamepad and (event is InputEventKey or event is InputEventMouseButton):
			return event_label(event)
	return "?"


## A keyboard or mouse event's name, as the menu shows it.
func event_label(event: InputEvent) -> String:
	if event is InputEventMouseButton:
		var button := (event as InputEventMouseButton).button_index
		return MOUSE_BUTTON_NAMES.get(button, "Mouse %d" % button)
	if event is InputEventKey:
		var key := event as InputEventKey
		var code := key.physical_keycode if key.physical_keycode != KEY_NONE else key.keycode
		if not _headless() and key.physical_keycode != KEY_NONE:
			var label := DisplayServer.keyboard_get_label_from_physical(key.physical_keycode)
			if label != KEY_NONE:
				code = label
		return OS.get_keycode_string(code)
	return "?"


## The keyboard or mouse event bound to `action`, or null, even while the
## keyboard and mouse are locked out.
func binding(action: StringName) -> InputEvent:
	for event: InputEvent in action_events(action):
		if event is InputEventKey or event is InputEventMouseButton:
			return event
	return null


## Binds `action` to a key or mouse button, in place of its current keyboard
## or mouse binding. An action that had the same key gets this one's old key,
## so nothing is left unbound.
func rebind(action: StringName, event: InputEvent) -> void:
	if not REBINDABLE.has(action):
		push_error("'%s' cannot be rebound." % action)
		return
	var value: Variant = _event_to_value(event)
	if value == null:
		return
	var old := binding(action)
	for other: StringName in REBINDABLE:
		var current := binding(other)
		if other != action and current != null and _event_to_value(current) == value:
			key_bindings[other] = null if old == null else _event_to_value(old)
	key_bindings[action] = value
	_rebuild_input_map()
	_mark_dirty()
	changed.emit()


func _set_binding(action: StringName, value: Variant) -> void:
	for event: InputEvent in InputMap.action_get_events(action):
		if event is InputEventKey or event is InputEventMouseButton:
			InputMap.action_erase_event(action, event)
	var event := _value_to_event(value)
	if event != null:
		InputMap.action_add_event(action, event)
	key_bindings[action] = value


## A key or mouse button as `["key", physical keycode]` or `["mouse", button]`,
## or null for anything else.
static func _event_to_value(event: InputEvent) -> Variant:
	if event is InputEventKey:
		var key := event as InputEventKey
		var code := key.physical_keycode if key.physical_keycode != KEY_NONE else key.keycode
		return ["key", int(code)]
	if event is InputEventMouseButton:
		return ["mouse", int((event as InputEventMouseButton).button_index)]
	return null


static func _value_to_event(value: Variant) -> InputEvent:
	if not (value is Array and value.size() == 2 and value[1] is int):
		return null
	match value[0]:
		"key":
			var key := InputEventKey.new()
			key.physical_keycode = value[1]
			return key
		"mouse":
			var mouse := InputEventMouseButton.new()
			mouse.button_index = value[1]
			return mouse
	return null


## Puts every action back to the project's bindings, applies `key_bindings`,
## then takes the unused device's bindings off the gameplay actions.
func _rebuild_input_map() -> void:
	InputMap.load_from_project_settings()
	var stored := key_bindings.duplicate()
	key_bindings.clear()
	for action: Variant in stored:
		if REBINDABLE.has(action) and InputMap.has_action(action):
			_set_binding(action, stored[action])
	_lock_input_device()


func _lock_input_device() -> void:
	_all_events.clear()
	var controller := uses_controller()
	for action: StringName in InputMap.get_actions():
		if action in SHARED_ACTIONS or String(action).begins_with("ui_"):
			continue
		var events := InputMap.action_get_events(action)
		_all_events[action] = events
		for event: InputEvent in events:
			var from_controller := is_controller_event(event)
			var from_desk := event is InputEventKey or event is InputEventMouseButton
			if (from_controller and not controller) or (from_desk and controller):
				InputMap.action_erase_event(action, event)
		# A key or button held through the switch must not stay held.
		if Input.is_action_pressed(action):
			Input.action_release(action)


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
	master_server = directory_address(OS.get_cmdline_user_args())
	key_bindings.clear()
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
	# First launch with a controller plugged in: play with it.
	if (
		not (err == OK and config.has_section_key("controls", "input_device"))
		and not _headless()
		and not Input.get_connected_joypads().is_empty()
	):
		input_device = InputDevice.CONTROLLER
	if err == OK:
		_migrate_sensitivity(config)
	master_server = directory_address(OS.get_cmdline_user_args())
	key_bindings.clear()
	if err == OK and config.has_section("keys"):
		for action: String in config.get_section_keys("keys"):
			key_bindings[StringName(action)] = config.get_value("keys", action)
	_dirty = false
	_save_timer.stop()
	apply_all()
	changed.emit()


## Reads sensitivity saved by older builds. Look speed saved as a multiplier
## becomes the nearest level, and mouse sensitivity saved as a level becomes its
## multiplier. Either old default becomes the current default.
func _migrate_sensitivity(config: ConfigFile) -> void:
	if not config.has_section_key("gamepad", &"stick_sensitivity_level"):
		var old: Variant = config.get_value("gamepad", LEGACY_STICK_SENSITIVITY, null)
		if (old is int or old is float) and not is_nan(float(old)):
			stick_sensitivity_level = (
				DEFAULTS[&"stick_sensitivity_level"]
				if is_equal_approx(float(old), 1.0)
				else sensitivity_level(float(old))
			)
	if not config.has_section_key("controls", &"mouse_sensitivity"):
		var level: Variant = config.get_value("controls", LEGACY_MOUSE_LEVEL, null)
		if level is int and level in SENSITIVITY_LEVEL_OPTIONS:
			mouse_sensitivity = (
				DEFAULTS[&"mouse_sensitivity"] if level == 7 else sensitivity_scale(level)
			)


func save_settings() -> Error:
	var config := ConfigFile.new()
	for section: String in SECTIONS:
		for key: StringName in SECTIONS[section]:
			config.set_value(section, key, get(key))
	for action: StringName in key_bindings:
		config.set_value("keys", action, key_bindings[action])
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


## How wide positional sounds pan in the current audio mix. `AudioEnvironment`
## asks for it when it starts.
func panning_strength() -> float:
	return MIX_PANNING[audio_mix]


func _apply(key: StringName) -> void:
	if BUSES.has(key):
		var bus := AudioServer.get_bus_index(BUSES[key])
		if bus >= 0:
			var volume: float = get(key)
			var offset: float = MIX_BUS_OFFSETS[audio_mix].get(BUSES[key], 0.0)
			AudioServer.set_bus_volume_db(bus, linear_to_db(volume) + offset)
			AudioServer.set_bus_mute(bus, volume <= 0.0)
		return
	match key:
		&"input_device":
			_rebuild_input_map()
		&"audio_mix":
			for volume_key: StringName in BUSES:
				_apply(volume_key)
			_apply_mix_compression()
			if is_inside_tree():
				get_tree().call_group(
					&"audio_environment", &"set_panning_strength", panning_strength()
				)
		&"graphics_quality", &"render_scale", &"motion_blur", &"lens_effects":
			if is_inside_tree():
				GraphicsQuality.apply(get_tree(), graphics_quality, render_scale, motion_blur)
			if _lens != null:
				_lens.configure(graphics_quality, lens_effects)
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


## Turns the Master bus's mix compressor on for mixes that use it.
func _apply_mix_compression() -> void:
	for i in AudioServer.get_bus_effect_count(0):
		if AudioServer.get_bus_effect(0, i).resource_name == "MixCompressor":
			AudioServer.set_bus_effect_enabled(0, i, MIX_COMPRESSION[audio_mix])


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
			return int(value) if number and int(value) in INT_OPTIONS[key] else default
		TYPE_FLOAT:
			if not number or is_nan(float(value)):
				return default
			var limits: Vector2 = RANGES[key]
			return clampf(float(value), limits.x, limits.y)
		TYPE_STRING:
			if value is String or value is StringName:
				var text := String(value).strip_edges().left(16 if key == &"player_name" else 255)
				return text if not text.is_empty() else default
	return default


func _on_node_added(node: Node) -> void:
	if node is WorldEnvironment or node is DirectionalLight3D:
		GraphicsQuality.adopt(node, graphics_quality)
	elif _lens != null and node is SoldierRig:
		# Contact shadows are for the eye: the dedicated server has no lens and
		# skips them.
		# Deferred: the rig is still entering the tree.
		Grounding.attach.call_deferred(node as Node3D)


func _mark_dirty() -> void:
	_dirty = true
	if _save_timer.is_inside_tree():
		_save_timer.start()


func _headless() -> bool:
	return DisplayServer.get_name() == "headless"
