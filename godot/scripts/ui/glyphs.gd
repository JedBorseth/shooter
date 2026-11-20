extends RefCounted
## Button prompt glyphs: controller buttons, in an Xbox and a PlayStation set,
## and mouse buttons, as images (Kenney's Input Prompts, CC0, in
## `assets/ui/glyphs/`). Keyboard keys stay as drawn key caps.
##
## A prompt names its key with a token inside square brackets, as every
## prompt string already does ("[F] Take M870"): a keyboard key is its name
## ("F", "Space"), a controller input is `pad:<button>` ("pad:a", "pad:rt"),
## and a mouse button is `mouse:<button>` ("mouse:left"). `Settings.key_label`
## and `Settings.prompt_key` make the tokens; `HudPaint` draws them; Labels
## that cannot draw images use `plain_text`.
##
## Pad tokens use Xbox's layout names for the position on the pad, whatever
## the family: "pad:a" is the bottom face button, a Cross on PlayStation.
##
## Load it with `preload`; everything here is static.

enum Family { XBOX, PLAYSTATION }

const DIR := "res://assets/ui/glyphs/"
const PAD := "pad:"
const MOUSE := "mouse:"
const FAMILY_DIRS: Array[String] = ["xbox", "playstation"]

## Pad token names by `JoyButton`.
const JOY_BUTTONS := {
	JOY_BUTTON_A: "a",
	JOY_BUTTON_B: "b",
	JOY_BUTTON_X: "x",
	JOY_BUTTON_Y: "y",
	JOY_BUTTON_BACK: "back",
	JOY_BUTTON_START: "start",
	JOY_BUTTON_LEFT_STICK: "ls",
	JOY_BUTTON_RIGHT_STICK: "rs",
	JOY_BUTTON_LEFT_SHOULDER: "lb",
	JOY_BUTTON_RIGHT_SHOULDER: "rb",
	JOY_BUTTON_DPAD_UP: "dpad_up",
	JOY_BUTTON_DPAD_DOWN: "dpad_down",
	JOY_BUTTON_DPAD_LEFT: "dpad_left",
	JOY_BUTTON_DPAD_RIGHT: "dpad_right",
}
## Pad token names by `JoyAxis`: triggers, and the sticks as a whole.
const JOY_AXES := {
	JOY_AXIS_LEFT_X: "stick_l",
	JOY_AXIS_LEFT_Y: "stick_l",
	JOY_AXIS_RIGHT_X: "stick_r",
	JOY_AXIS_RIGHT_Y: "stick_r",
	JOY_AXIS_TRIGGER_LEFT: "lt",
	JOY_AXIS_TRIGGER_RIGHT: "rt",
}
const MOUSE_BUTTONS := {
	MOUSE_BUTTON_LEFT: "left",
	MOUSE_BUTTON_RIGHT: "right",
	MOUSE_BUTTON_MIDDLE: "middle",
	MOUSE_BUTTON_WHEEL_UP: "wheel_up",
	MOUSE_BUTTON_WHEEL_DOWN: "wheel_down",
	MOUSE_BUTTON_XBUTTON1: "side_back",
	MOUSE_BUTTON_XBUTTON2: "side_forward",
}

## Each pad button's name in text, per family.
const PAD_NAMES: Array[Dictionary] = [
	{
		"a": "A",
		"b": "B",
		"x": "X",
		"y": "Y",
		"back": "View",
		"start": "Menu",
		"ls": "LS",
		"rs": "RS",
		"lb": "LB",
		"rb": "RB",
		"lt": "LT",
		"rt": "RT",
		"dpad_up": "D-pad up",
		"dpad_down": "D-pad down",
		"dpad_left": "D-pad left",
		"dpad_right": "D-pad right",
		"stick_l": "Left stick",
		"stick_r": "Right stick",
	},
	{
		"a": "Cross",
		"b": "Circle",
		"x": "Square",
		"y": "Triangle",
		"back": "Create",
		"start": "Options",
		"ls": "L3",
		"rs": "R3",
		"lb": "L1",
		"rb": "R1",
		"lt": "L2",
		"rt": "R2",
		"dpad_up": "D-pad up",
		"dpad_down": "D-pad down",
		"dpad_left": "D-pad left",
		"dpad_right": "D-pad right",
		"stick_l": "Left stick",
		"stick_r": "Right stick",
	},
]
const MOUSE_NAMES := {
	"left": "LMB",
	"right": "RMB",
	"middle": "MMB",
	"wheel_up": "Wheel up",
	"wheel_down": "Wheel down",
	"side_back": "Mouse 4",
	"side_forward": "Mouse 5",
}

## Sony's USB vendor id, for telling a PlayStation pad from its SDL GUID.
const SONY_VENDOR := "4c05"

## The set controller glyphs draw from. `Settings` keeps it current (the
## button prompts setting, or the connected controller).
static var family: Family = Family.XBOX

static var _textures := {}


## True if `key` is a token drawn as an image (a pad or mouse button).
static func is_glyph(key: String) -> bool:
	return key.begins_with(PAD) or key.begins_with(MOUSE)


static func is_pad(key: String) -> bool:
	return key.begins_with(PAD)


## The token for a controller button or axis event, or "".
static func pad_token(event: InputEvent) -> String:
	if event is InputEventJoypadButton:
		var name: String = JOY_BUTTONS.get((event as InputEventJoypadButton).button_index, "")
		return PAD + name if name != "" else ""
	if event is InputEventJoypadMotion:
		var axis: String = JOY_AXES.get((event as InputEventJoypadMotion).axis, "")
		return PAD + axis if axis != "" else ""
	return ""


## The token for a mouse button, or "".
static func mouse_token(button: MouseButton) -> String:
	var name: String = MOUSE_BUTTONS.get(button, "")
	return MOUSE + name if name != "" else ""


## The image for a glyph token in the current family, or null.
static func texture(key: String) -> Texture2D:
	var path := ""
	if key.begins_with(PAD):
		path = DIR + FAMILY_DIRS[family] + "/" + key.substr(PAD.length()) + ".svg"
	elif key.begins_with(MOUSE):
		path = DIR + "mouse/" + key.substr(MOUSE.length()) + ".svg"
	else:
		return null
	if not _textures.has(path):
		_textures[path] = load(path) as Texture2D if ResourceLoader.exists(path) else null
	return _textures[path]


## A token's name in words ("Cross", "RT", "LMB"); a keyboard key as it is.
static func plain(key: String) -> String:
	if key.begins_with(PAD):
		return String(PAD_NAMES[family].get(key.substr(PAD.length()), key))
	if key.begins_with(MOUSE):
		return String(MOUSE_NAMES.get(key.substr(MOUSE.length()), key))
	return key


## `text` with every "[token]" written in words, for Labels and Buttons.
static func plain_text(text: String) -> String:
	var out := ""
	for part: Array in segments(text):
		out += ("[%s]" % plain(part[1])) if part[0] else part[1]
	return out


## Labels `button` "<label>  [key]", or "<label>" with the button's glyph
## beside it for a controller or mouse button.
static func key_button(button: Button, label: String, key: String) -> void:
	var glyph := texture(key) if is_glyph(key) else null
	button.text = label if glyph != null else "%s  [%s]" % [label, key]
	button.icon = glyph
	button.expand_icon = glyph != null
	button.icon_alignment = HORIZONTAL_ALIGNMENT_RIGHT


## Splits text into [is_key, string] parts at each "[KEY]".
static func segments(text: String) -> Array:
	var parts := []
	var at := 0
	while at < text.length():
		var open := text.find("[", at)
		var close := text.find("]", open + 1) if open >= 0 else -1
		if open < 0 or close < 0:
			parts.append([false, text.substr(at)])
			break
		if open > at:
			parts.append([false, text.substr(at, open - at)])
		parts.append([true, text.substr(open + 1, close - open - 1)])
		at = close + 1
	return parts


## The family a connected controller belongs to, from its name and GUID.
static func family_of(device: int) -> Family:
	var name := Input.get_joy_name(device).to_lower()
	for hint: String in ["playstation", "dualsense", "dualshock", "ps4", "ps5", "sony"]:
		if name.contains(hint):
			return Family.PLAYSTATION
	# SDL GUIDs carry the USB vendor id, little-endian, from the ninth digit.
	var guid := Input.get_joy_guid(device).to_lower()
	if guid.length() >= 12 and guid.substr(8, 4) == SONY_VENDOR:
		return Family.PLAYSTATION
	return Family.XBOX
