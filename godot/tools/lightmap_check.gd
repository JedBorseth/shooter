class_name LightmapCheck
## Tells a healthy baked lightmap from a failed one. The editor's baker can
## report success and still write an all-black lightmap when it loses the GPU
## partway (another process was rendering at the same time); a black lightmap
## turns the whole map dark and nothing else notices. Used after every bake
## (tools/check_lightmap.gd) and in CI (tests/lightmaps_test.gd).

## Lightmaps are atlases with empty space between charts (and whole empty
## quadrants on small maps), so a healthy one is far from all lit: about half
## its texels are. A failed bake has none, or few.
const MIN_LIT := 0.05
const MIN_LIT_QUADRANTS := 2
const SAMPLES := 64


## Why the lightmap beside `scene` (its .exr) looks failed, or "" if it looks
## healthy. Missing lightmaps are not checked here.
static func problem(scene: String) -> String:
	var path := scene.get_basename() + ".exr"
	var image := Image.load_from_file(ProjectSettings.globalize_path(path))
	if image == null or image.is_empty():
		return "%s could not be read" % path
	var lit := 0
	var quadrants := PackedInt32Array([0, 0, 0, 0])
	for i in SAMPLES:
		for j in SAMPLES:
			var c := image.get_pixel(
				int((i + 0.5) * image.get_width() / SAMPLES),
				int((j + 0.5) * image.get_height() / SAMPLES)
			)
			if c.r + c.g + c.b > 0.001:
				lit += 1
				quadrants[int(i * 2 / SAMPLES) + 2 * int(j * 2 / SAMPLES)] += 1
	var share := float(lit) / (SAMPLES * SAMPLES)
	var lit_quadrants := 0
	for q in quadrants:
		if float(q) / (SAMPLES * SAMPLES / 4) >= MIN_LIT:
			lit_quadrants += 1
	if share < MIN_LIT or lit_quadrants < MIN_LIT_QUADRANTS:
		return (
			"%s looks black: %.1f%% of samples lit, %d quadrant(s) lit"
			% [path, share * 100.0, lit_quadrants]
		)
	return ""
