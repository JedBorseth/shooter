extends RefCounted
## Controller button layouts, stick layouts, and the trigger flip, as Call of
## Duty offers them (Black Ops 2 to Black Ops 6, Modern Warfare 2019 on).
## `Settings` calls `apply` after it reloads the input map, which moves the
## controller half of each gameplay action to where the chosen layout puts it.
## Keyboard and mouse bindings are left alone.
##
## A layout only moves the ten face, shoulder, trigger, and stick-click
## inputs, as Call of Duty's presets do. The D-pad (scorestreaks, loadout,
## inspect), View (scoreboard), and Menu (pause) keep the project's bindings.
##
## Load it with `preload`; everything here is static.

const Glyphs := preload("res://scripts/ui/glyphs.gd")

## The triggers, which Godot reads as axes rather than buttons. A layout names
## an input with a `JoyButton` or one of these.
const LT := -10
const RT := -11

## The inputs a layout fills, in the order the menu lists them.
const INPUTS: Array[int] = [
	RT,
	LT,
	JOY_BUTTON_RIGHT_SHOULDER,
	JOY_BUTTON_LEFT_SHOULDER,
	JOY_BUTTON_A,
	JOY_BUTTON_B,
	JOY_BUTTON_X,
	JOY_BUTTON_Y,
	JOY_BUTTON_LEFT_STICK,
	JOY_BUTTON_RIGHT_STICK,
]

## The actions a layout places, one input each.
const ACTIONS: Array[StringName] = [
	&"fire",
	&"aim",
	&"lethal",
	&"tactical",
	&"jump",
	&"crouch",
	&"reload",
	&"weapon_next",
	&"sprint",
	&"melee",
]

## Each preset: its name, and the input each action in ACTIONS sits on.
## Default is Black Ops' (and this project's) standard layout. The rest move
## actions from it the way Call of Duty's presets of the same name do.
const BUTTON_LAYOUTS: Array[Dictionary] = [
	{
		"name": "Default",
		"map":
		{
			&"fire": RT,
			&"aim": LT,
			&"lethal": JOY_BUTTON_RIGHT_SHOULDER,
			&"tactical": JOY_BUTTON_LEFT_SHOULDER,
			&"jump": JOY_BUTTON_A,
			&"crouch": JOY_BUTTON_B,
			&"reload": JOY_BUTTON_X,
			&"weapon_next": JOY_BUTTON_Y,
			&"sprint": JOY_BUTTON_LEFT_STICK,
			&"melee": JOY_BUTTON_RIGHT_STICK,
		},
	},
	# Crouch on the right stick click, so you slide and drop without taking a
	# thumb off aiming; melee moves to B.
	{
		"name": "Tactical",
		"map":
		{
			&"fire": RT,
			&"aim": LT,
			&"lethal": JOY_BUTTON_RIGHT_SHOULDER,
			&"tactical": JOY_BUTTON_LEFT_SHOULDER,
			&"jump": JOY_BUTTON_A,
			&"crouch": JOY_BUTTON_RIGHT_STICK,
			&"reload": JOY_BUTTON_X,
			&"weapon_next": JOY_BUTTON_Y,
			&"sprint": JOY_BUTTON_LEFT_STICK,
			&"melee": JOY_BUTTON_B,
		},
	},
	# Jump on the left bumper, so you jump without leaving the right stick;
	# the tactical takes A.
	{
		"name": "Bumper Jumper",
		"map":
		{
			&"fire": RT,
			&"aim": LT,
			&"lethal": JOY_BUTTON_RIGHT_SHOULDER,
			&"tactical": JOY_BUTTON_A,
			&"jump": JOY_BUTTON_LEFT_SHOULDER,
			&"crouch": JOY_BUTTON_B,
			&"reload": JOY_BUTTON_X,
			&"weapon_next": JOY_BUTTON_Y,
			&"sprint": JOY_BUTTON_LEFT_STICK,
			&"melee": JOY_BUTTON_RIGHT_STICK,
		},
	},
	# Bumper Jumper with Tactical's crouch on the right stick click.
	{
		"name": "Bumper Jumper Tactical",
		"map":
		{
			&"fire": RT,
			&"aim": LT,
			&"lethal": JOY_BUTTON_RIGHT_SHOULDER,
			&"tactical": JOY_BUTTON_A,
			&"jump": JOY_BUTTON_LEFT_SHOULDER,
			&"crouch": JOY_BUTTON_RIGHT_STICK,
			&"reload": JOY_BUTTON_X,
			&"weapon_next": JOY_BUTTON_Y,
			&"sprint": JOY_BUTTON_LEFT_STICK,
			&"melee": JOY_BUTTON_B,
		},
	},
	# The triggers and bumpers trade sides: fire on the left trigger. With the
	# Southpaw stick layout it mirrors the whole pad for left-handed players.
	{
		"name": "Lefty",
		"map":
		{
			&"fire": LT,
			&"aim": RT,
			&"lethal": JOY_BUTTON_LEFT_SHOULDER,
			&"tactical": JOY_BUTTON_RIGHT_SHOULDER,
			&"jump": JOY_BUTTON_A,
			&"crouch": JOY_BUTTON_B,
			&"reload": JOY_BUTTON_X,
			&"weapon_next": JOY_BUTTON_Y,
			&"sprint": JOY_BUTTON_LEFT_STICK,
			&"melee": JOY_BUTTON_RIGHT_STICK,
		},
	},
]

## Stick layouts: which stick moves and which looks. Legacy splits them the
## way the first console shooters did: the left stick walks forward and back
## and turns, the right stick strafes and looks up and down. The Southpaw
## layouts swap the sticks' clicks with them, so sprint stays on the stick
## that walks forward and melee on the one that aims.
const STICK_LAYOUTS: Array[String] = ["Default", "Southpaw", "Legacy", "Legacy Southpaw"]
const STICK_SWAPS_CLICKS: Array[bool] = [false, true, false, true]

## Per stick layout, the axis each of move X, move Y, look X, and look Y reads.
const STICK_AXES: Array[Array] = [
	[JOY_AXIS_LEFT_X, JOY_AXIS_LEFT_Y, JOY_AXIS_RIGHT_X, JOY_AXIS_RIGHT_Y],
	[JOY_AXIS_RIGHT_X, JOY_AXIS_RIGHT_Y, JOY_AXIS_LEFT_X, JOY_AXIS_LEFT_Y],
	[JOY_AXIS_RIGHT_X, JOY_AXIS_LEFT_Y, JOY_AXIS_LEFT_X, JOY_AXIS_RIGHT_Y],
	[JOY_AXIS_LEFT_X, JOY_AXIS_RIGHT_Y, JOY_AXIS_RIGHT_X, JOY_AXIS_LEFT_Y],
]

## The stick actions, with the axis slot (as in STICK_AXES) and direction
## each reads.
const STICK_ACTIONS := {
	&"move_left": [0, -1.0],
	&"move_right": [0, 1.0],
	&"move_forward": [1, -1.0],
	&"move_back": [1, 1.0],
	&"look_left": [2, -1.0],
	&"look_right": [2, 1.0],
	&"look_up": [3, -1.0],
	&"look_down": [3, 1.0],
}

## Controls no layout moves, shown under the layout's buttons: the sticks
## (by the action that names each half of a stick layout), then the D-pad,
## View, and Menu.
const FIXED_ACTIONS: Array[StringName] = [
	&"move_forward",
	&"move_left",
	&"look_right",
	&"look_up",
	&"loadout",
	&"inspect",
	&"streak_next",
	&"streak_use",
	&"scoreboard",
	&"pause",
]

## Each action's name on the controller's button map. The reload button also
## uses what is in front of you (a weapon to pick up, the bomb).
const PAD_ACTION_NAMES := {
	&"fire": "Fire",
	&"aim": "Aim down sights",
	&"lethal": "Lethal (frag grenade)",
	&"tactical": "Tactical (flashbang or smoke)",
	&"jump": "Jump / mantle",
	&"crouch": "Crouch / slide (hold: prone)",
	&"reload": "Reload / interact",
	&"weapon_next": "Switch weapon",
	&"sprint": "Sprint",
	&"melee": "Melee",
	&"move_forward": "Move forward and back",
	&"move_left": "Strafe",
	&"look_right": "Turn",
	&"look_up": "Look up and down",
	&"loadout": "Loadout",
	&"inspect": "Inspect weapon",
	&"streak_next": "Next scorestreak",
	&"streak_use": "Use scorestreak",
	&"scoreboard": "Scoreboard",
	&"pause": "Menu",
}


## The glyph token for an input ("pad:rt"), as `Glyphs` draws it.
static func token(input: int) -> String:
	match input:
		LT:
			return "pad:lt"
		RT:
			return "pad:rt"
	return "pad:" + String(Glyphs.JOY_BUTTONS.get(input, ""))


static func layout_names() -> Array[String]:
	var names: Array[String] = []
	for layout: Dictionary in BUTTON_LAYOUTS:
		names.append(layout["name"])
	return names


## The input `action` sits on under a button layout, with the trigger flip
## and the stick layout's click swap applied: a `JoyButton`, LT, RT, or -1
## when the layout does not place it.
static func input_for(action: StringName, layout: int, flip := false, stick := 0) -> int:
	var map: Dictionary = BUTTON_LAYOUTS[clampi(layout, 0, BUTTON_LAYOUTS.size() - 1)]["map"]
	var input: int = map.get(action, -1)
	if flip:
		input = flipped(input)
	if STICK_SWAPS_CLICKS[clampi(stick, 0, STICK_SWAPS_CLICKS.size() - 1)]:
		match input:
			JOY_BUTTON_LEFT_STICK:
				input = JOY_BUTTON_RIGHT_STICK
			JOY_BUTTON_RIGHT_STICK:
				input = JOY_BUTTON_LEFT_STICK
	return input


## Flip Triggers swaps each bumper with the trigger on its side, as Call of
## Duty's "Flip L1/L2 with R1/R2" (Black Ops' "Flipped" layouts) does.
static func flipped(input: int) -> int:
	match input:
		LT:
			return JOY_BUTTON_LEFT_SHOULDER
		RT:
			return JOY_BUTTON_RIGHT_SHOULDER
		JOY_BUTTON_LEFT_SHOULDER:
			return LT
		JOY_BUTTON_RIGHT_SHOULDER:
			return RT
	return input


## The action on `input` under a layout, or &"" for none.
static func action_on(input: int, layout: int, flip := false, stick := 0) -> StringName:
	for action: StringName in ACTIONS:
		if input_for(action, layout, flip, stick) == input:
			return action
	return &""


## The event for an input: a button press, or a trigger pulled.
static func event_for(input: int) -> InputEvent:
	if input == LT or input == RT:
		var motion := InputEventJoypadMotion.new()
		motion.axis = JOY_AXIS_TRIGGER_LEFT if input == LT else JOY_AXIS_TRIGGER_RIGHT
		motion.axis_value = 1.0
		return motion
	var button := InputEventJoypadButton.new()
	button.button_index = input as JoyButton
	return button


## Moves the controller half of the gameplay actions onto `layout`, `stick`,
## and `flip`. Run it on a fresh input map, before the unused device is
## locked out. Returns the actions now on a trigger, for the trigger deadzone.
static func apply(layout: int, stick: int, flip: bool) -> Array[StringName]:
	var on_trigger: Array[StringName] = []
	for action: StringName in ACTIONS:
		if not InputMap.has_action(action):
			continue
		_erase_pad(action)
		var input := input_for(action, layout, flip, stick)
		if input == -1:
			continue
		InputMap.action_add_event(action, event_for(input))
		if input == LT or input == RT:
			on_trigger.append(action)
	var axes: Array = STICK_AXES[clampi(stick, 0, STICK_AXES.size() - 1)]
	for action: StringName in STICK_ACTIONS:
		if not InputMap.has_action(action):
			continue
		_erase_pad(action)
		var motion := InputEventJoypadMotion.new()
		motion.axis = axes[STICK_ACTIONS[action][0]]
		motion.axis_value = STICK_ACTIONS[action][1]
		InputMap.action_add_event(action, motion)
	return on_trigger


static func _erase_pad(action: StringName) -> void:
	for event: InputEvent in InputMap.action_get_events(action):
		if event is InputEventJoypadButton or event is InputEventJoypadMotion:
			InputMap.action_erase_event(action, event)
