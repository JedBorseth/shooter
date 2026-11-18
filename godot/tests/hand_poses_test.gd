extends "res://tests/bake_hand_poses.gd"
## Headless test that every weapon's finger poses (`hands` in its weapon file)
## are the fit tests/bake_hand_poses.gd makes from its model today, and that
## no fitted finger or palm goes into the weapon. Fails when a model, a grip
## marker, or the gloves changed without re-running the bake.
##   godot --headless --path godot --script res://tests/hand_poses_test.gd


func _initialize() -> void:
	check = true
	super._initialize()
