extends SceneTree
## Captures prestige emblems where other players see them, for the docs: the
## scoreboard and the kill feed with names at several prestiges. Needs a GPU
## and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_prestige.gd

const Scoreboard := preload("res://scripts/net/scoreboard.gd")
const KillFeed := preload("res://scripts/net/kill_feed.gd")
const TeamColours := preload("res://scripts/ui/team_colours.gd")

var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	var layer := CanvasLayer.new()
	root.add_child(layer)
	var backdrop := ColorRect.new()
	backdrop.color = Color(0.16, 0.18, 0.2)
	backdrop.set_anchors_preset(Control.PRESET_FULL_RECT)
	layer.add_child(backdrop)
	var board: Control = Scoreboard.new()
	board.set_anchors_preset(Control.PRESET_FULL_RECT)
	layer.add_child(board)
	var ally := TeamColours.of(true)
	var enemy := TeamColours.of(false)
	var rows := []
	for player: Array in [
		["Vega", 11, 2450, 21, 9, true], ["Rook", 7, 1900, 16, 10], ["Slam", 3, 1300, 11, 12]
	]:
		(
			rows
			. append(
				{
					"cells": [player[0], str(player[2]), str(player[3]), str(player[4]), "38"],
					"me": player.size() > 5,
					"prestige": player[1],
				}
			)
		)
	var them := []
	for player: Array in [
		["Kite", 10, 2100, 18, 11], ["Moth", 5, 1500, 12, 14], ["Pike", 0, 900, 7, 15]
	]:
		(
			them
			. append(
				{
					"cells": [player[0], str(player[2]), str(player[3]), str(player[4]), "41"],
					"prestige": player[1],
				}
			)
		)
	var headings := ["NAME", "SCORE", "KILLS", "DEATHS", "PING"]
	board.set("title", "TEAM DEATHMATCH")
	(
		board
		. set(
			"sections",
			[
				{
					"name": "YOUR TEAM",
					"score": "54",
					"colour": ally,
					"headings": headings,
					"rows": rows
				},
				{
					"name": "ENEMY",
					"score": "49",
					"colour": enemy,
					"headings": headings,
					"rows": them
				},
			]
		)
	)
	var feed: Control = KillFeed.new()
	feed.position = Vector2(40, 560)
	feed.size = Vector2(600, 300)
	layer.add_child(feed)
	feed.call(
		"add",
		[{"prestige": 11}, ["Vega", ally], {"icon": "m4"}, {"prestige": 10}, ["Kite", enemy]],
		true
	)
	feed.call(
		"add",
		[
			{"prestige": 5},
			["Moth", enemy],
			{"icon": "ak47"},
			{"headshot": true},
			{"prestige": 7},
			["Rook", ally]
		]
	)
	feed.call("add", [{"prestige": 3}, ["Slam", ally], {"icon": "m870"}, ["Pike", enemy]])
	for i in 20:
		await process_frame
	await RenderingServer.frame_post_draw
	root.get_texture().get_image().save_png(_out.path_join("prestige_scoreboard.png"))
	print("saved prestige_scoreboard")
	quit(0)
