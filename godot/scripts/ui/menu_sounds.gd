extends Node
## The Options and pause menu's click and hover sounds (`pause_menu.gd`). Both
## are optional until the files are added to the project.

const CLICK_SOUND := "res://assets/audio/ui/click.ogg"
const HOVER_SOUND := "res://assets/audio/ui/hover.ogg"
const UI_BUS := &"UI"

## While true, focus moves without a hover sound (the menu moving focus
## itself, not the player).
var quiet := false
var _click: AudioStreamPlayer
var _hover: AudioStreamPlayer


## Sounds every button under `root` on press, hover, and focus, and each
## slider in `sliders` on focus.
func attach(root: Control, sliders: Array) -> void:
	_click = _sound_player(CLICK_SOUND)
	_hover = _sound_player(HOVER_SOUND)
	for node: Node in root.find_children("*", "BaseButton", true, false):
		var button := node as BaseButton
		button.pressed.connect(_play.bind(_click))
		button.mouse_entered.connect(_play.bind(_hover))
		button.focus_entered.connect(_on_focus)
	for slider: HSlider in sliders:
		slider.focus_entered.connect(_on_focus)


func _sound_player(path: String) -> AudioStreamPlayer:
	if not ResourceLoader.exists(path):
		return null
	var stream := load(path) as AudioStream
	if stream == null:
		return null
	var player := AudioStreamPlayer.new()
	player.stream = stream
	player.bus = UI_BUS
	player.max_polyphony = 3
	add_child(player)
	return player


func _on_focus() -> void:
	# Keyboard and gamepad navigation only: a click already plays its own sound.
	if not quiet and not Input.is_mouse_button_pressed(MOUSE_BUTTON_LEFT):
		_play(_hover)


func _play(player: AudioStreamPlayer) -> void:
	if player != null:
		player.play()
