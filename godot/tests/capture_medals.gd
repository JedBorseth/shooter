extends SceneTree
## Captures the score feed: kill popups, award lines, and each medal banner at
## a few points in its animation, for reviewing the medal UI.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --script res://tests/capture_medals.gd

const AWARDS := [
	[
		"double_kill",
		[
			[100, "Kill", false],
			[50, "Headshot", false],
			[100, "Kill", false],
			[50, "Double Kill", true]
		]
	],
	["triple_kill", [[100, "Kill", false], [75, "Triple Kill", true]]],
	["quad_kill", [[100, "Kill", false], [100, "Quad Kill", true]]],
	["multi_kill", [[100, "Kill", false], [150, "5x Multi Kill", true]]],
	["collateral", [[100, "Kill", false], [100, "Kill", false], [50, "Collateral", true]]],
	["streak_5", [[100, "Kill", false], [25, "Longshot", false], [250, "5 Kill Streak", true]]],
	["streak_10", [[100, "Kill", false], [500, "10 Kill Streak", true]]],
]

var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	var main: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(main)
	main.get_node("Sentry").set("enabled", false)
	var player := main.get_node("Player") as FpsPlayer
	player.set_look(0.0, -0.05)
	var feed := main.get_node("HUD/ScoreFeed")
	(feed.get_node("MedalSound") as AudioStreamPlayer).volume_db = -80.0
	await _seconds(1.0)
	for entry: Array in AWARDS:
		for award: Array in entry[1]:
			feed.add_award(award[0], award[1], award[2])
		await _seconds(0.1)
		await _shot("%s_a_in" % entry[0])
		await _seconds(0.5)
		await _shot("%s_b_hold" % entry[0])
		await _seconds(3.0)
	quit()


func _seconds(time: float) -> void:
	await create_timer(time).timeout


func _shot(name: String) -> void:
	await process_frame
	await RenderingServer.frame_post_draw
	var path := _out.path_join(name + ".png")
	root.get_viewport().get_texture().get_image().save_png(path)
	print("saved ", path)
