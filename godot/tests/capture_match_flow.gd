extends SceneTree
## Captures the screens around the action in a match, filled with made-up
## data over a still of the game, for design review: the scoreboard in team,
## round, and free-for-all modes, the countdown, the result banners with the
## team strip and match bonus, the results board with the match report, and
## the round and call banners. Fast: no match runs.
## Needs a GPU and a display (or Xvfb). BACKDROP is a 1600x900 still (a
## `clean` capture from `capture_ui.gd`); without it a dark fill stands in.
##   CAPTURE_DIR=/path/to/out BACKDROP=/path/to/still.png godot --path godot \
##     --resolution 1600x900 --audio-driver Dummy --script res://tests/capture_match_flow.gd

const Scoreboard := preload("res://scripts/net/scoreboard.gd")
const MatchBanner := preload("res://scripts/net/match_banner.gd")
const XpSummary := preload("res://scripts/net/xp_summary.gd")
const TeamColours := preload("res://scripts/ui/team_colours.gd")
const UiTheme := preload("res://scripts/ui/ui_theme.gd")

const NAMES := ["Hollis", "Bot 2", "Player", "Bot 4", "Marlow", "Bot 6"]
const ENEMIES := ["Bot 1", "Okafor", "Bot 3", "Reyes", "Bot 5", "Lindqvist"]

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
	await _boards()
	await _banners()
	await _results()
	await _deaths()
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


func _piece(script: GDScript) -> Control:
	var piece := Control.new()
	piece.set_script(script)
	piece.set_anchors_preset(Control.PRESET_FULL_RECT)
	piece.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_layer.add_child(piece)
	return piece


func _team_rows(names: Array, me: int, out: Array, wide := false) -> Array:
	var rows := []
	for i in names.size():
		var bot := String(names[i]).begins_with("Bot")
		var score := 1450 - i * 210
		var cells := [names[i], str(score)]
		if wide:
			cells.append(str((7 - i) % 3))
			cells.append(str(i % 2))
		cells.append(str(12 - i * 2))
		cells.append(str(4 + i))
		cells.append("—" if bot else str([24, 61, 142, 33, 230, 18][i]))
		rows.append({"cells": cells, "me": i == me, "faint": i in out, "tag": "BOT" if bot else ""})
	return rows


func _sections(wide: bool, out_ours: Array, out_theirs: Array) -> Array:
	var headings := ["Name", "Score", "Kills", "Deaths", "Ping"]
	if wide:
		headings = ["Name", "Score", "Plants", "Defuses", "Kills", "Deaths", "Ping"]
	return [
		{
			"name": "Your team",
			"score": "4" if wide else "62",
			"colour": TeamColours.ally,
			"team": 0,
			"headings": headings,
			"rows": _team_rows(NAMES, 2, out_ours, wide),
		},
		{
			"name": "Enemy",
			"score": "3" if wide else "57",
			"colour": TeamColours.enemy,
			"team": 1,
			"headings": headings,
			"rows": _team_rows(ENEMIES, -1, out_theirs, wide),
		},
	]


func _boards() -> void:
	var board: Control = _piece(Scoreboard)
	board.title = "TEAM DEATHMATCH"
	board.clock = "7:32"
	board.sections = _sections(false, [], [])
	await _seconds(0.6)
	await _shot("board_01_tdm")
	board.title = "SEARCH AND DESTROY"
	board.clock = "1:48"
	board.sections = _sections(true, [1, 4], [0, 2, 3])
	await _frames(3)
	await _shot("board_02_snd")
	var rows := []
	var everyone := NAMES + ENEMIES
	for i in 8:
		var bot := String(everyone[i]).begins_with("Bot")
		(
			rows
			. append(
				{
					"cells":
					[
						everyone[i],
						str(1800 - i * 190),
						str(18 - i * 2),
						str(3 + i),
						"—" if bot else "45"
					],
					"me": i == 2,
					"tag": "BOT" if bot else "",
				}
			)
		)
	board.title = "FREE-FOR-ALL"
	board.clock = "4:05"
	board.sections = [
		{
			"name": "Every player for themselves",
			"score": "",
			"colour": UiTheme.TEXT,
			"headings": ["Name", "Score", "Kills", "Deaths", "Ping"],
			"rows": rows,
		}
	]
	await _frames(3)
	await _shot("board_03_ffa")
	board.queue_free()


func _banners() -> void:
	var banner: Control = _piece(MatchBanner)
	banner.countdown = 2.6
	banner.countdown_mode = "Team Deathmatch"
	banner.countdown_objective = "Eliminate the enemy team  ·  first to 75 kills"
	await _frames(3)
	await _shot("banner_01_countdown")
	banner.countdown = 2.93
	await _frames(2)
	await _shot("banner_01b_countdown_pop")
	banner.countdown = 4.4
	banner.countdown_mode = "Round 3  ·  Attack"
	banner.countdown_objective = "Plant the charge at a site"
	await _frames(2)
	await _shot("banner_02_round_countdown")
	banner.countdown = 0.0
	banner.title = "ROUND WON"
	banner.detail = "Enemies eliminated"
	banner.colour = TeamColours.ally
	banner.strip = {
		"left": {"name": "Your team", "score": 3, "team": 0, "colour": TeamColours.ally},
		"right": {"name": "Enemy", "score": 2, "team": 1, "colour": TeamColours.enemy},
	}
	await _seconds(1.0)
	await _shot("banner_03_round_won")
	banner.title = "ROUND LOST"
	banner.detail = "The charge went off"
	banner.colour = TeamColours.enemy
	await _seconds(1.0)
	await _shot("banner_04_round_lost")
	banner.title = ""
	banner.strip = {}
	banner.callout = "HALFTIME"
	banner.callout_detail = "Switching sides"
	banner.callout_colour = UiTheme.TEXT
	await _seconds(1.0)
	await _shot("banner_05_callout")
	banner.callout = "OVERTIME"
	banner.callout_detail = "Next capture wins"
	banner.callout_colour = UiTheme.ACCENT
	await _seconds(0.12)
	await _shot("banner_06_callout_intro")
	banner.queue_free()


func _results() -> void:
	var board: Control = _piece(Scoreboard)
	board.visible = false
	var banner: Control = _piece(MatchBanner)
	var strip := {
		"left": {"name": "Your team", "score": 75, "team": 0, "colour": TeamColours.ally},
		"right": {"name": "Enemy", "score": 61, "team": 1, "colour": TeamColours.enemy},
	}
	banner.banner_y = 0.09
	banner.title = "VICTORY"
	banner.plate = UiTheme.VICTORY
	banner.detail = "Score limit reached"
	banner.footnote = "Next match in 24"
	banner.strip = strip
	banner.bonus = 1500
	await _seconds(0.16)
	await _shot("result_01_victory_intro")
	await _seconds(1.4)
	await _shot("result_02_victory_strip")
	banner.title = "DEFEAT"
	banner.plate = UiTheme.DEFEAT
	banner.detail = "Time limit reached"
	strip.left.score = 37
	strip.right.score = 52
	banner.bonus = 650
	banner.strip = {}
	await _frames(2)
	banner.strip = strip
	await _seconds(1.6)
	await _shot("result_03_defeat_strip")
	banner.title = "DRAW"
	banner.plate = Color.TRANSPARENT
	banner.colour = UiTheme.TEXT
	banner.detail = "Time limit reached"
	strip.left.score = 40
	strip.right.score = 40
	banner.bonus = 900
	await _seconds(1.2)
	await _shot("result_04_draw")
	# The results board under the banner, with the match report.
	banner.title = "VICTORY"
	banner.plate = UiTheme.VICTORY
	banner.detail = "Score limit reached"
	banner.strip = {}
	banner.bonus = -1
	board.visible = true
	board.top = 214.0
	board.title = "TEAM DEATHMATCH"
	board.title_colour = UiTheme.TEXT_DIM
	board.sections = _sections(false, [], [])
	var card: PanelContainer = XpSummary.new()
	card.set_anchors_preset(Control.PRESET_CENTER_BOTTOM)
	card.offset_left = -XpSummary.WIDTH / 2.0
	card.offset_right = XpSummary.WIDTH / 2.0
	card.offset_top = -236.0
	card.offset_bottom = -52.0
	card.grow_vertical = Control.GROW_DIRECTION_BEGIN
	_layer.add_child(card)
	(
		card
		. show_summary(
			{
				"score": 1030,
				"bonus": 1500,
				"outcome": "win",
				"xp_before": 10020,
				"xp_after": 12550,
				"kills": {"m4": 12, "m1911": 2},
				"unlocks": Progression.unlocks_at(8),
			}
		)
	)
	await _seconds(0.9)
	await _shot("result_05_board_filling")
	await _seconds(1.6)
	await _shot("result_06_board_report")


## The death card, then the killcams: after a death, a round's last kill, and
## the match's.
func _deaths() -> void:
	for child in _layer.get_children():
		if not child is TextureRect:
			child.queue_free()
	# Loaded here: they reach autoloads, which a main script cannot preload.
	var card: Control = _piece(load("res://scripts/hud_centre.gd"))
	card.dead = true
	card.killer = "Okafor"
	card.killer_colour = TeamColours.enemy
	card.killer_weapon = "ak47"
	card.respawn_total = 5.0
	card.respawn_left = 2.4
	await _seconds(0.6)
	await _shot("death_01_card")
	card.queue_free()
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
	await _frames(1)
	await _shot("killcam_01_cut")
	await _seconds(0.8)
	await _shot("killcam_02_respawn")
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
	await _shot("killcam_03_final")
	cam.set("preview", {})
	await _frames(2)
	(
		cam
		. set(
			"preview",
			{
				"kind": "round",
				"killer": 7,
				"me": 1,
				"killer_name": "Okafor",
				"killer_team": 1,
				"victim_team": 0,
				"my_team": 0,
				"victim": 3,
				"victim_name": "Hollis",
				"weapon": "M870",
				"attachment": "Ext. Mag",
				"progress": 0.2,
				"skippable": false,
			}
		)
	)
	await _seconds(0.8)
	await _shot("killcam_04_round")
	cam.queue_free()


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
