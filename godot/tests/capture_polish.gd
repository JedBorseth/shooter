extends SceneTree
## Captures the small screens and states the UI polish pass touched, filled
## with made-up data over a still of the game, for design review: the
## loading screen's team band, the death card for each way to die (a gun, a
## knife, a frag, a rocket, the charge, your own frag, a fall, unknown), the
## killcam's name cards and weapon row (a gun, a knife), the scoreboard's
## grouped numbers, and the slot being edited in Create-a-Class while its
## items have focus. Fast: no match runs.
## Needs a GPU and a display (or Xvfb). BACKDROP is a 1600x900 still (a
## `clean` capture from `capture_ui.gd`); without it a dark fill stands in.
##   CAPTURE_DIR=/path/to/out BACKDROP=/path/to/still.png godot --path godot \
##     --resolution 1600x900 --audio-driver Dummy --script res://tests/capture_polish.gd

const Scoreboard := preload("res://scripts/net/scoreboard.gd")
const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")
const MatchPaint := preload("res://scripts/ui/match_paint.gd")

const DEATHS := [
	["death_01_gun", "Okafor", 1, "ak47", "AK-47", false, false],
	["death_02_knife", "Okafor", 1, "", "Knife", false, false],
	["death_03_frag", "Lindqvist", 1, "", "Frag", false, false],
	["death_04_rocket", "Reyes", 1, "", "Rocket", false, false],
	["death_05_charge", "Okafor", 1, "", "Charge", false, false],
	["death_06_own_frag", "Player", 0, "", "Frag", true, false],
	["death_07_fall", "", -1, "", "", false, true],
	["death_08_unknown", "", -1, "", "", false, false],
]

var _out := OS.get_environment("CAPTURE_DIR")
var _layer: CanvasLayer


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	_layer = CanvasLayer.new()
	root.add_child(_layer)
	_backdrop()
	await _loading()
	await _deaths()
	await _killcams()
	await _board()
	_clear()
	await _class_editing()
	quit(0)


func _backdrop() -> void:
	var still := OS.get_environment("BACKDROP")
	var rect := TextureRect.new()
	rect.set_anchors_preset(Control.PRESET_FULL_RECT)
	rect.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
	rect.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	if still != "" and FileAccess.file_exists(still):
		rect.texture = ImageTexture.create_from_image(Image.load_from_file(still))
	_layer.add_child(rect)


func _clear() -> void:
	for child in _layer.get_children():
		if not child is TextureRect:
			child.queue_free()


func _piece(script: GDScript) -> Control:
	var piece := Control.new()
	piece.set_script(script)
	piece.set_anchors_preset(Control.PRESET_FULL_RECT)
	piece.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_layer.add_child(piece)
	return piece


func _loading() -> void:
	for shot: Array in [["tdm", 0, "loading_01_tdm"], ["snd", 1, "loading_02_snd_team1"]]:
		var screen: Control = load("res://scripts/ui/loading_screen.gd").new()
		screen.set("map", "breakwater")
		screen.set("mode", shot[0])
		screen.set("team", shot[1])
		screen.set("progress", 0.45)
		screen.set("status", "Connecting to Local match  ·  0:02")
		_layer.add_child(screen)
		await _seconds(0.8)
		await _shot(shot[2])
		screen.queue_free()
		await _frames(2)


func _deaths() -> void:
	for death: Array in DEATHS:
		# Loaded here: it reaches autoloads, which a main script cannot preload.
		var card: Control = _piece(load("res://scripts/hud_centre.gd"))
		card.dead = true
		card.killer = death[1]
		var own: bool = death[5]
		card.killer_colour = TeamColours.ally if own else TeamColours.enemy
		card.killer_team = death[2]
		card.killer_weapon = death[3]
		card.killer_cause = death[4]
		card.killer_self = own
		card.fell = death[6]
		card.respawn_total = 5.0
		card.respawn_left = 2.4
		await _seconds(0.6)
		await _shot(death[0])
		card.queue_free()
		await _frames(2)


func _killcams() -> void:
	var cam := CanvasLayer.new()
	cam.set_script(load("res://scripts/net/killcam_hud.gd"))
	(
		cam
		. set(
			"preview",
			{
				"kind": "respawn",
				"killer": 7,
				"killer_name": "Okafor",
				"killer_team": 1,
				"victim_team": 0,
				"my_team": 0,
				"victim": 1,
				"me": 1,
				"victim_name": "Player",
				"weapon": "AK-47",
				"attachment": "Red Dot",
				"headshot": true,
				"progress": 0.4,
				"skippable": true,
			}
		)
	)
	root.add_child(cam)
	await _seconds(0.8)
	await _shot("killcam_01_respawn")
	cam.set("preview", {})
	await _frames(2)
	(
		cam
		. set(
			"preview",
			{
				"kind": "final",
				"killer": 1,
				"me": 1,
				"killer_name": "Player",
				"killer_team": 0,
				"victim_team": 1,
				"my_team": 0,
				"victim": 7,
				"victim_name": "Okafor",
				"weapon": "M4A1",
				"attachment": "",
				"progress": 0.7,
				"skippable": false,
			}
		)
	)
	await _seconds(0.8)
	await _shot("killcam_02_final")
	cam.set("preview", {})
	await _frames(2)
	(
		cam
		. set(
			"preview",
			{
				"kind": "respawn",
				"killer": 7,
				"killer_name": "Okafor",
				"killer_team": 1,
				"victim_team": 0,
				"my_team": 0,
				"victim": 1,
				"me": 1,
				"victim_name": "Player",
				"weapon": "Knife",
				"attachment": "",
				"progress": 0.5,
				"skippable": true,
			}
		)
	)
	await _seconds(0.8)
	await _shot("killcam_03_knife")
	cam.queue_free()
	await _frames(2)


## The team scoreboard with scores past a thousand, grouped as the match
## report groups them.
func _board() -> void:
	_clear()
	var board: Control = _piece(Scoreboard)
	board.title = "TEAM DEATHMATCH"
	board.clock = "4:12"
	var sections := []
	for side in 2:
		var names: Array = (
			["Hollis", "Bot 2", "Player", "Bot 4"] if side == 0 else ["Okafor", "Bot 3", "Reyes"]
		)
		var rows := []
		for i in names.size():
			var bot := String(names[i]).begins_with("Bot")
			var score := 2450 - i * 470 - side * 300
			(
				rows
				. append(
					{
						"cells":
						[
							names[i],
							MatchPaint.number(score),
							str(14 - i * 3),
							str(4 + i),
							"—" if bot else str([24, 61, 33, 18][i]),
						],
						"me": side == 0 and i == 2,
						"tag": "",
					}
				)
			)
		(
			sections
			. append(
				{
					"name": "Your team" if side == 0 else "Enemy",
					"score": MatchPaint.number([1030, 940][side]),
					"colour": TeamColours.of(side == 0),
					"team": side,
					"headings": ["Name", "Score", "Kills", "Deaths", "Ping"],
					"rows": rows,
				}
			)
		)
	board.sections = sections
	await _seconds(0.8)
	await _shot("board_01_grouped")


## Create-a-Class's editor with focus in the items: the slot being edited
## keeps its mark.
func _class_editing() -> void:
	ClassStore.use_file("user://capture_polish_classes.cfg")
	ProgressStore.use_file("user://capture_polish_progress.cfg")
	for file in [ClassStore.path(), ProgressStore.path()]:
		DirAccess.remove_absolute(ProjectSettings.globalize_path(file))
	ClassStore.reload()
	ProgressStore.reload()
	ProgressStore.add(48000, {"m4": 60, "pdw9": 100})
	ClassStore.reload()
	Input.warp_mouse(Vector2.ZERO)
	var menu: Control = load("res://scripts/net/match_menu.gd").new()
	menu.set("mode_name", "Team Deathmatch")
	_layer.add_child(menu)
	await _frames(8)
	var classes: Control = menu.get("class_menu")
	classes.call("focus_first")
	await _frames(4)
	await _shot("class_01_list")
	classes.call("_open_editor", "custom:0")
	await _frames(6)
	for slot: String in ["primary", "lethal", "perk1", "wildcards"]:
		var tile: Button = (classes.get("_grid") as Control).call("tile", slot)
		tile.grab_focus()
		await _frames(4)
		var picker: Control = classes.get("_picker")
		var card: Button = picker.call("card", 1)
		if card == null:
			card = picker.call("card", 0)
		card.grab_focus()
		await _seconds(0.4)
		await _shot("class_02_editing_" + slot)
	for file in [ClassStore.path(), ProgressStore.path()]:
		DirAccess.remove_absolute(ProjectSettings.globalize_path(file))


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _seconds(seconds: float) -> void:
	await create_timer(seconds).timeout


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	root.get_viewport().get_texture().get_image().save_png(_out.path_join(name + ".png"))
	print("saved ", name)
