extends SceneTree
## Fails if a baked lightmap came out black. The editor's baker can report
## success and still write an all-zero lightmap when it loses the GPU partway
## (another process was rendering at the same time), and a black lightmap turns
## the whole map dark. tools/bake_lightmaps.sh runs this after every bake:
##   godot --headless --path godot --script res://tools/check_lightmap.gd -- \
##     res://scenes/slamcity.tscn ...
## Each scene's lightmap is the .exr beside it with the same name.

## Fraction of sampled texels that must be lit. Lightmaps are atlases with
## empty space between charts, so a healthy one is far from all lit, but a
## failed bake has none at all.
const MIN_LIT := 0.02
const SAMPLES := 64


func _initialize() -> void:
	var failures := 0
	for scene: String in OS.get_cmdline_user_args():
		var path := scene.get_basename() + ".exr"
		if not FileAccess.file_exists(path):
			print("check_lightmap: %s has no lightmap, skipped" % scene)
			continue
		var image := Image.load_from_file(ProjectSettings.globalize_path(path))
		if image == null or image.is_empty():
			print("check_lightmap: FAIL %s could not be read" % path)
			failures += 1
			continue
		var lit := 0
		for i in SAMPLES:
			for j in SAMPLES:
				var x := int((i + 0.5) * image.get_width() / SAMPLES)
				var y := int((j + 0.5) * image.get_height() / SAMPLES)
				var c := image.get_pixel(x, y)
				if c.r + c.g + c.b > 0.001:
					lit += 1
		var share := float(lit) / (SAMPLES * SAMPLES)
		if share < MIN_LIT:
			print(
				"check_lightmap: FAIL %s is black (%.1f%% of samples lit)" % [path, share * 100.0]
			)
			failures += 1
		else:
			print("check_lightmap: %s ok (%.0f%% of samples lit)" % [path, share * 100.0])
	quit(1 if failures > 0 else 0)
