extends "res://tests/bake_hand_poses.gd"
## Headless test that every weapon has fitted hand poses (`hands` in its
## weapon file, from tests/bake_hand_poses.gd) and that, held in them, no
## sampled finger or palm goes into the weapon further than the bake's limits
## and listed exceptions allow. It does not compare the poses with a fresh fit,
## so a changed model passes as long as the stored poses still clear it.
##   godot --headless --path godot --script res://tests/hand_poses_test.gd


func _initialize() -> void:
	check = true
	super._initialize()
