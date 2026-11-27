extends SceneTree
## Turns a map's visible CSG blockout into static meshes with lightmap UVs, so
## LightmapGI can bake it. For every visible CSG root it saves the baked mesh to
## res://assets/maps/<scene>/<node>.res, adds a MeshInstance3D beside it, and
## hides the CSG node. The hidden CSG keeps its collision and its `surface`
## metadata, so movement and impacts do not change.
##
## Run again after editing the CSG: nodes this tool hid are shown and rebaked,
## and the meshes it added are replaced. CSG that was hidden on purpose (the
## prop blockers) is left alone.
##   godot --headless --path godot --script res://tools/bake_map_meshes.gd -- \
##     res://scenes/main.tscn ...
## tools/bake_lightmaps.sh runs this before it bakes.

## World-space size of one lightmap texel, in metres.
const TEXEL_SIZE := 0.2
const BAKED := &"csg_baked"
const VISUAL := &"csg_visual"
const GraphicsQuality := preload("res://scripts/graphics.gd")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	# The scenes are saved again: keep graphics presets and looks out of them.
	GraphicsQuality.managed = false
	# Keep bake-only lights (scripts/baked_lights.gd) while the scenes are re-saved.
	Engine.set_meta(&"keep_baked_lights", true)
	var failures := 0
	for path in OS.get_cmdline_user_args():
		if not await _bake_scene(path):
			failures += 1
	quit(1 if failures > 0 else 0)


func _bake_scene(path: String) -> bool:
	var packed := load(path) as PackedScene
	if packed == null:
		push_error("bake_map_meshes: cannot load %s" % path)
		return false
	var root := packed.instantiate()
	root_add(root)
	for i in 3:
		await process_frame
	# Undo an earlier run.
	for node in root.find_children("*", "MeshInstance3D", true, false):
		if node.has_meta(VISUAL) and node.owner == root:
			node.get_parent().remove_child(node)
			node.free()
	for node in root.find_children("*", "CSGShape3D", true, false):
		if node.has_meta(BAKED):
			(node as CSGShape3D).visible = true
			node.remove_meta(BAKED)

	var folder := "res://assets/maps/%s" % path.get_file().get_basename()
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(folder))
	var count := 0
	for node in root.find_children("*", "CSGShape3D", true, false):
		var csg := node as CSGShape3D
		if not csg.is_root_shape() or not csg.is_visible_in_tree() or csg.owner != root:
			continue
		# Let CSG rebuild a shape that was just shown again.
		await process_frame
		var mesh := csg.bake_static_mesh()
		if mesh == null or mesh.get_surface_count() == 0:
			continue
		var err := mesh.lightmap_unwrap(csg.global_transform, TEXEL_SIZE)
		if err != OK:
			push_error("bake_map_meshes: unwrap failed for %s (%s)" % [csg.name, error_string(err)])
			return false
		var mesh_path := "%s/%s.res" % [folder, String(root.get_path_to(csg)).replace("/", "_")]
		mesh.resource_name = String(csg.name)
		ResourceSaver.save(mesh, mesh_path)
		mesh.take_over_path(mesh_path)
		var visual := MeshInstance3D.new()
		visual.name = "%sMesh" % csg.name
		visual.mesh = load(mesh_path)
		visual.transform = csg.transform
		visual.gi_mode = GeometryInstance3D.GI_MODE_STATIC
		visual.set_meta(VISUAL, true)
		csg.get_parent().add_child(visual)
		csg.get_parent().move_child(visual, csg.get_index() + 1)
		visual.owner = root
		csg.visible = false
		csg.set_meta(BAKED, true)
		count += 1

	var out := PackedScene.new()
	var err := out.pack(root)
	if err == OK:
		err = ResourceSaver.save(out, path)
	root.get_parent().remove_child(root)
	root.free()
	print("bake_map_meshes: %s: %d meshes (%s)" % [path, count, error_string(err)])
	return err == OK


func root_add(node: Node) -> void:
	# CSG builds its mesh only inside the tree (bake_static_mesh updates it).
	root.add_child(node)
