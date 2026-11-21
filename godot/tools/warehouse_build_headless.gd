extends SceneTree
## Runs the Warehouse builder without the editor and saves the scene:
##   WAREHOUSE_PARTS=machines,stock,clip xvfb-run -a godot --path godot \
##     --script res://tools/warehouse_build_headless.gd
## WAREHOUSE_PARTS defaults to the entire generated layout.
## Run it with a real renderer (a display, or Xvfb), not --headless: the
## headless renderer keeps no MultiMesh instances, so the girders, joists,
## rods, trays, fixtures, and strewn clutter would be saved empty. It refuses to.

const LEVEL := "res://scenes/warehouse.tscn"
const Builder := preload("res://tools/warehouse_photo_build.gd")
const MULTIMESH_PARTS := ["structure", "lighting", "utilities", "detail"]


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var text := OS.get_environment("WAREHOUSE_PARTS")
	var parts: Array = (
		Array(text.split(",", false))
		if not text.is_empty()
		else [
			"shell",
			"structure",
			"lighting",
			"utilities",
			"machines",
			"stock",
			"patches",
			"boxes",
			"detail",
			"effects",
			"clip",
			"gameplay",
		]
	)
	var multimesh_parts := parts.filter(func(p: String) -> bool: return p in MULTIMESH_PARTS)
	if DisplayServer.get_name() == "headless" and not multimesh_parts.is_empty():
		push_error(
			(
				"%s build MultiMeshes, which a headless run saves empty. Run under a display or Xvfb."
				% [multimesh_parts]
			)
		)
		quit(1)
		return
	# The bake-only lights free themselves outside the editor
	# (scripts/baked_lights.gd); keep them, or the saved scene loses them and
	# the next bake comes out black.
	Engine.set_meta(&"keep_baked_lights", true)
	var source := load(LEVEL) as PackedScene
	if source == null:
		push_error("could not load the Warehouse scene")
		quit(1)
		return
	var scene: Node3D = source.instantiate()
	# Keep the player and everything else still while the builder works: the
	# scene is saved as it stands.
	scene.process_mode = Node.PROCESS_MODE_DISABLED
	root.add_child(scene)
	await process_frame
	print(Builder.build(scene, parts))
	scene.process_mode = Node.PROCESS_MODE_INHERIT
	var packed := PackedScene.new()
	var err := packed.pack(scene)
	if err != OK:
		push_error("pack failed: %d" % err)
		quit(1)
		return
	var uid := ResourceLoader.get_resource_uid(LEVEL)
	err = ResourceSaver.save(packed, LEVEL)
	if err == OK:
		_restore_uids(uid)
	print("saved ", LEVEL, " ", err)
	quit(0 if err == OK else 1)


## A script run writes no UIDs, so the scene would lose its own and every
## external resource's. Put them back so the editor keeps resolving moved files.
## It also writes each instanced scene's root type, which the editor leaves out
## (the instance gives it): drop it, so a rebuild's diff is only what changed.
func _restore_uids(scene_uid: int) -> void:
	var lines := FileAccess.get_file_as_string(LEVEL).split("\n")
	var path_re := RegEx.create_from_string('path="([^"]+)"')
	var type_re := RegEx.create_from_string(' type="[A-Za-z0-9_]+"')
	for i in lines.size():
		var line := lines[i]
		if line.begins_with("[node ") and line.contains(" instance=ExtResource("):
			line = type_re.sub(line, "")
			lines[i] = line
		if i == 0 and scene_uid != ResourceUID.INVALID_ID and not line.contains("uid="):
			lines[i] = line.trim_suffix("]") + ' uid="%s"]' % ResourceUID.id_to_text(scene_uid)
		elif line.begins_with("[ext_resource") and not line.contains("uid="):
			var m := path_re.search(line)
			var id := (
				ResourceLoader.get_resource_uid(m.get_string(1)) if m else ResourceUID.INVALID_ID
			)
			if id != ResourceUID.INVALID_ID:
				lines[i] = line.replace(' path="', ' uid="%s" path="' % ResourceUID.id_to_text(id))
	var f := FileAccess.open(LEVEL, FileAccess.WRITE)
	f.store_string("\n".join(lines))
	f.close()
