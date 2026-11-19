extends "res://tests/bake_support_sections.gd"
## Headless test that every two-handed weapon's file stores the section of its
## third-person model where the support hand holds it (`section` in its grips,
## from tests/bake_support_sections.gd), within the bake's tolerance, so the
## drawn support palm and the server's arm hit zones rest on the handguard
## the model has.
##   godot --headless --path godot --script res://tests/support_sections_test.gd


func _initialize() -> void:
	check = true
	super._initialize()
