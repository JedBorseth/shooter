class_name LightmapCheck
## Tells a healthy baked lightmap from a failed one. Used after every bake
## (tools/check_lightmap.gd) and in CI (tests/lightmaps_test.gd). It catches
## two failures:
##
## - The editor's baker can report success and still write an all-black
##   lightmap when it loses the GPU partway (another process was rendering at
##   the same time); a black lightmap turns the whole map dark.
## - A bake can also come out black on some surfaces only (#240: the Testing
##   Area's walls and floor, while its props stayed lit). The sun is drawn
##   live, so sunlit ground still looks right, and only the shade shows it: it
##   is the lightmap's bounce and sky light, and there it turns black. The
##   atlas as a whole still has plenty of lit texels from the other meshes, so
##   this is checked per mesh, on the texels each mesh's lightmap UVs cover.

## Lightmaps are atlases with empty space between charts (and whole empty
## quadrants on small maps), so a healthy one is far from all lit: about half
## its texels are. A failed bake has none, or few.
const MIN_LIT := 0.05
const MIN_LIT_QUADRANTS := 2
const SAMPLES := 64
## A texel this dark (r + g + b) gets no light at all.
const BLACK := 0.001
## The most of a map's surface, by lightmap texels, that may get no light.
## Healthy bakes leave 6-16% black (faces against walls and the ground, the
## undersides of things) in 2026-11; #240's failure left about 80%.
const MAX_BLACK := 0.3
## A mesh holding this much of a map's lightmap texels is a main surface
## (the architecture, the ground, a city block), and must itself be mostly
## lit: in healthy bakes the worst such mesh is 25% black.
const MAIN_MESH := 0.05
const MAX_MAIN_BLACK := 0.4


## Why the lightmap beside `scene` (its .exr) looks failed, or "" if it looks
## healthy. Missing lightmaps are not checked here.
static func problem(scene: String) -> String:
	var path := scene.get_basename() + ".exr"
	var image := Image.load_from_file(ProjectSettings.globalize_path(path))
	if image == null or image.is_empty():
		return "%s could not be read" % path
	var black := atlas_problem(image)
	if not black.is_empty():
		return "%s %s" % [path, black]
	return surface_problem(scene, image)


## Why `image`, a lightmap atlas, looks all black, or "".
static func atlas_problem(image: Image) -> String:
	var lit := 0
	var quadrants := PackedInt32Array([0, 0, 0, 0])
	for i in SAMPLES:
		for j in SAMPLES:
			var c := image.get_pixel(
				int((i + 0.5) * image.get_width() / SAMPLES),
				int((j + 0.5) * image.get_height() / SAMPLES)
			)
			if c.r + c.g + c.b > BLACK:
				lit += 1
				quadrants[int(i * 2 / SAMPLES) + 2 * int(j * 2 / SAMPLES)] += 1
	var share := float(lit) / (SAMPLES * SAMPLES)
	var lit_quadrants := 0
	for q in quadrants:
		if float(q) / (SAMPLES * SAMPLES / 4) >= MIN_LIT:
			lit_quadrants += 1
	if share < MIN_LIT or lit_quadrants < MIN_LIT_QUADRANTS:
		return (
			"looks black: %.1f%% of samples lit, %d quadrant(s) lit"
			% [share * 100.0, lit_quadrants]
		)
	return ""


## Why `image`, `scene`'s lightmap atlas, leaves too much of the map's
## surface black, or "".
static func surface_problem(scene: String, image: Image) -> String:
	var shares := surface_shares(scene, image)
	if shares.is_empty():
		return "%s: no lightmapped meshes found" % scene
	var total := 0.0
	var black := 0.0
	for mesh: String in shares:
		total += shares[mesh].x
		black += shares[mesh].y
	var worst := ""
	var worst_share := 0.0
	for mesh: String in shares:
		var texels: float = shares[mesh].x
		var share: float = shares[mesh].y / maxf(texels, 1.0)
		if texels >= total * MAIN_MESH and share > worst_share:
			worst = mesh
			worst_share = share
	if black / total > MAX_BLACK or worst_share > MAX_MAIN_BLACK:
		return (
			(
				"%s's lightmap is black on %.0f%% of its surface (at most %.0f%%), "
				+ "and on %.0f%% of %s (at most %.0f%%)"
			)
			% [
				scene,
				black / total * 100.0,
				MAX_BLACK * 100.0,
				worst_share * 100.0,
				worst,
				MAX_MAIN_BLACK * 100.0,
			]
		)
	return ""


## For each mesh baked into `scene`'s lightmap (by its path from the
## LightmapGI): Vector2(the lightmap texels its UVs cover, how many of them
## are black), read from `image`, the lightmap atlas. Each triangle counts
## its texel area, sampled at its centre.
static func surface_shares(scene: String, image: Image) -> Dictionary:
	var out := {}
	var packed := load(scene) as PackedScene
	if packed == null:
		return out
	var root := packed.instantiate()
	var found := root.find_children("*", "LightmapGI", true, false)
	if found.is_empty() or (found[0] as LightmapGI).light_data == null:
		root.free()
		return out
	var gi := found[0] as LightmapGI
	var data := gi.light_data
	var textures: Array = data.get("lightmap_textures")
	var layers := (textures[0] as TextureLayered).get_layers() if not textures.is_empty() else 1
	# The atlas's layers are stacked top to bottom in the image.
	var size := Vector2(image.get_width(), image.get_height() / layers)
	var users: Array = data.get("user_data")
	for u in range(0, users.size(), 4):
		var node := gi.get_node_or_null(NodePath(users[u]))
		# Sub-instances (multimeshes, grid maps) are not checked.
		if not node is MeshInstance3D or int(users[u + 3]) >= 0:
			continue
		var mesh := (node as MeshInstance3D).mesh
		if mesh == null:
			continue
		var rect: Rect2 = users[u + 1]
		var top := int(users[u + 2]) * int(size.y)
		var texels := 0.0
		var black := 0.0
		for s in mesh.get_surface_count():
			var arrays := mesh.surface_get_arrays(s)
			var uv2: PackedVector2Array = arrays[Mesh.ARRAY_TEX_UV2]
			if uv2.is_empty():
				continue
			var index: PackedInt32Array = arrays[Mesh.ARRAY_INDEX]
			if index.is_empty():
				index = PackedInt32Array(range(uv2.size()))
			for t in range(0, index.size() - 2, 3):
				var a := (rect.position + uv2[index[t]] * rect.size) * size
				var b := (rect.position + uv2[index[t + 1]] * rect.size) * size
				var c := (rect.position + uv2[index[t + 2]] * rect.size) * size
				var area := absf((b - a).cross(c - a)) * 0.5
				var at := (a + b + c) / 3.0
				var x := clampi(int(at.x), 0, int(size.x) - 1)
				var y := clampi(int(at.y), 0, int(size.y) - 1) + top
				var colour := image.get_pixel(x, y)
				texels += area
				if colour.r + colour.g + colour.b <= BLACK:
					black += area
		if texels > 0.0:
			var path := String(users[u])
			out[path] = out.get(path, Vector2.ZERO) + Vector2(texels, black)
	root.free()
	return out
