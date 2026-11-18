@tool
extends EditorScenePostImport
## Import script for the standalone models in assets/models/ (written by
## tools/build_models.py). Each surface's material is a placeholder named after
## a shared material in materials/models/; this swaps the real one in. The
## models are props that can move or vanish, so they take light from a map's
## lightmap probes rather than baking into it.


func _post_import(scene: Node) -> Object:
	var cache := {}
	for node in scene.find_children("*", "MeshInstance3D", true, false):
		var instance := node as MeshInstance3D
		var mesh := instance.mesh as ArrayMesh
		if mesh == null:
			continue
		for i in mesh.get_surface_count():
			var placeholder := mesh.surface_get_material(i)
			var key := placeholder.resource_name if placeholder != null else ""
			if not cache.has(key):
				cache[key] = _material(key)
			if cache[key] != null:
				mesh.surface_set_material(i, cache[key])
		instance.gi_mode = GeometryInstance3D.GI_MODE_DYNAMIC
	return scene


func _material(key: String) -> Material:
	var path := "res://materials/models/%s.tres" % key
	if ResourceLoader.exists(path):
		return load(path) as Material
	push_warning("model_import: no material named %s" % key)
	return null
