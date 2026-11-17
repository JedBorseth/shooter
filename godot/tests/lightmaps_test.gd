extends SceneTree
## Every map's committed lightmap is a real bake, not a black one: the editor's
## baker can report success and still write all zeros (tools/lightmap_check.gd).
## Run from the repo root: godot --headless --path godot --script res://tests/lightmaps_test.gd

const MAPS := [
	"res://scenes/main.tscn", "res://scenes/slamcity.tscn", "res://scenes/zombies_arena.tscn"
]


func _initialize() -> void:
	var failures := 0
	for scene: String in MAPS:
		var problem := LightmapCheck.problem(scene)
		print(
			(
				"%s %s's lightmap is lit%s"
				% [
					"PASS" if problem.is_empty() else "FAIL",
					scene,
					"" if problem.is_empty() else ": " + problem
				]
			)
		)
		if not problem.is_empty():
			failures += 1
	print("Lightmaps test: %d failure(s)" % failures)
	quit(1 if failures > 0 else 0)
