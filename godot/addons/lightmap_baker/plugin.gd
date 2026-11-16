@tool
extends EditorPlugin
## Bakes lightmaps without a person at the editor. When the editor starts with
## BAKE_LIGHTMAPS set to a comma list of scene paths, this opens each scene,
## selects its LightmapGI node, presses the editor's own "Bake Lightmaps"
## button, saves the scene, and quits. Without the variable it does nothing.
## Run it through tools/bake_lightmaps.sh, which needs a GPU (or Xvfb with one).


func _enter_tree() -> void:
	var scenes := OS.get_environment("BAKE_LIGHTMAPS")
	if scenes.is_empty():
		return
	_bake_all.call_deferred(scenes.split(",", false))


func _bake_all(scenes: PackedStringArray) -> void:
	# Let the editor finish starting and scanning the file system.
	await _frames(30)
	while EditorInterface.get_resource_filesystem().is_scanning():
		await _frames(5)
	var failures := 0
	for path in scenes:
		if not await _bake(path.strip_edges()):
			failures += 1
	print("lightmap baker: done, %d failure(s)" % failures)
	EditorInterface.get_editor_main_screen().get_tree().quit(1 if failures > 0 else 0)


func _bake(path: String) -> bool:
	print("lightmap baker: ", path)
	EditorInterface.open_scene_from_path(path)
	var root: Node = null
	# A big map takes a while to open the first time after its meshes change.
	for i in 600:
		await _frames(5)
		root = EditorInterface.get_edited_scene_root()
		if root != null and root.scene_file_path == path:
			break
	if root == null or root.scene_file_path != path:
		push_error("lightmap baker: could not open %s" % path)
		return false
	var lightmaps := root.find_children("*", "LightmapGI", true, false)
	if lightmaps.is_empty():
		push_error("lightmap baker: %s has no LightmapGI" % path)
		return false
	var lightmap := lightmaps[0] as LightmapGI
	# Without existing data the editor asks where to save it; give it a file.
	var data_path := path.get_basename() + ".lmbake"
	if lightmap.light_data == null or lightmap.light_data.resource_path.is_empty():
		if not FileAccess.file_exists(data_path):
			ResourceSaver.save(LightmapGIData.new(), data_path)
		lightmap.light_data = load(data_path) as LightmapGIData
	var selection := EditorInterface.get_selection()
	selection.clear()
	selection.add_node(lightmaps[0])
	EditorInterface.edit_node(lightmaps[0])
	await _frames(10)
	var button := _find_bake_button(EditorInterface.get_base_control())
	if button == null:
		push_error("lightmap baker: no Bake Lightmaps button")
		return false
	var started := Time.get_ticks_msec()
	var before := _texture_count(lightmap)
	button.pressed.emit()
	# The bake may run on a thread behind a progress dialog; wait for its textures.
	for i in 1200:
		await _frames(5)
		if (
			_texture_count(lightmap) > 0 and _texture_count(lightmap) != before
			or i > 4 and before > 0
		):
			break
	await _frames(30)
	print("lightmap baker: baked in %.1f s" % ((Time.get_ticks_msec() - started) / 1000.0))
	if _texture_count(lightmap) == 0:
		push_error("lightmap baker: the bake produced no textures for %s" % path)
		return false
	EditorInterface.save_scene()
	await _frames(10)
	return true


func _texture_count(lightmap: LightmapGI) -> int:
	if lightmap.light_data == null:
		return 0
	return lightmap.light_data.lightmap_textures.size()


func _find_bake_button(node: Node) -> Button:
	if node is Button and (node as Button).text.strip_edges() == "Bake Lightmaps":
		return node
	for child in node.get_children():
		var found := _find_bake_button(child)
		if found != null:
			return found
	return null


func _frames(count: int) -> void:
	for i in count:
		await get_tree().process_frame
