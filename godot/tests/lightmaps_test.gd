extends SceneTree
## Every map's committed lightmap is a real bake: not black, and not black on
## the map's main surfaces either (tools/lightmap_check.gd). Also checks the
## check: a copy of a healthy lightmap with the Testing Area's walls and
## floor blacked out (#240's failure, which the atlas-wide check passed) and
## an all-black copy must both fail.
## Run from the repo root: godot --headless --path godot --script res://tests/lightmaps_test.gd

const MAPS := [
	"res://scenes/main.tscn", "res://scenes/slamcity.tscn", "res://scenes/zombies_arena.tscn"
]
const TESTING := "res://scenes/main.tscn"

var _failures := 0


func _initialize() -> void:
	for scene: String in MAPS:
		var problem := LightmapCheck.problem(scene)
		_check(
			problem.is_empty(),
			"%s's lightmap is lit%s" % [scene, "" if problem.is_empty() else ": " + problem]
		)
	_check_failed_bakes()
	print("Lightmaps test: %d failure(s)" % _failures)
	quit(1 if _failures > 0 else 0)


func _check_failed_bakes() -> void:
	var image := Image.load_from_file(
		ProjectSettings.globalize_path(TESTING.get_basename() + ".exr")
	)
	# #240: the map's own meshes (walls, floor) came out black; the props did not.
	var data := (load(TESTING.get_basename() + ".lmbake")) as LightmapGIData
	var users: Array = data.get("user_data")
	var layers := (data.get("lightmap_textures")[0] as TextureLayered).get_layers()
	var height := image.get_height() / layers
	var shaded := image.duplicate() as Image
	var blacked := 0
	for u in range(0, users.size(), 4):
		if not String(users[u]).begins_with("../Architecture/"):
			continue
		var rect: Rect2 = users[u + 1]
		shaded.fill_rect(
			Rect2i(
				int(rect.position.x * image.get_width()),
				int(rect.position.y * height) + int(users[u + 2]) * height,
				ceili(rect.size.x * image.get_width()),
				ceili(rect.size.y * height)
			),
			Color.BLACK
		)
		blacked += 1
	_check(blacked == 2, "the Testing Area bakes its walls and its floor (%d meshes)" % blacked)
	var atlas := LightmapCheck.atlas_problem(shaded)
	var surface := LightmapCheck.surface_problem(TESTING, shaded)
	print(
		(
			"  walls and floor black: atlas check %s; surface check %s"
			% ["passes" if atlas.is_empty() else "fails", surface]
		)
	)
	_check(not surface.is_empty(), "a bake with black walls and floor fails")
	var black := Image.create_empty(
		image.get_width(), image.get_height(), false, image.get_format()
	)
	_check(not LightmapCheck.atlas_problem(black).is_empty(), "an all-black bake fails")


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1
