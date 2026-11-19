extends SceneTree
## Fails if a baked lightmap came out black, all over or on the map's main
## surfaces (see tools/lightmap_check.gd).
## tools/bake_lightmaps.sh runs it after every bake:
##   godot --headless --path godot --script res://tools/check_lightmap.gd -- \
##     res://scenes/slamcity.tscn ...


func _initialize() -> void:
	var failures := 0
	for scene: String in OS.get_cmdline_user_args():
		if not FileAccess.file_exists(scene.get_basename() + ".exr"):
			print("check_lightmap: %s has no lightmap, skipped" % scene)
			continue
		var problem := LightmapCheck.problem(scene)
		if problem.is_empty():
			print("check_lightmap: %s ok" % scene)
		else:
			print("check_lightmap: FAIL %s" % problem)
			failures += 1
	quit(1 if failures > 0 else 0)
