@tool
extends EditorScenePostImport
## Import script for Slamcity's meshes (assets/maps/slamcity/city.gltf, written
## by tools/build_slamcity_city.py). Each surface's material is a placeholder
## named after a shared material in materials/slamcity/ (or a look in
## materials/world/); this swaps the real one in. Chunks named `*_detail` hold
## small things: they cast no shadow, are hidden beyond 60 m, and take light
## from the lightmap's probes instead of lightmap texels. Chunks named
## `*_shadow` are shadow-only proxies (see scripts/slamcity.gd).

const DETAIL_RANGE := 60.0


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
		if String(instance.name) == "far_train":
			# Runs along the far elevated line (scripts/slamcity.gd): lit by
			# the lightmap's probes, casts no shadow, and stays out of the bake.
			instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			instance.gi_mode = GeometryInstance3D.GI_MODE_DISABLED
		elif String(instance.name).ends_with("_shadow"):
			# Hidden in the editor, so the lightmap bake uses the real meshes;
			# scripts/slamcity.gd shows it in the game.
			instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_SHADOWS_ONLY
			instance.gi_mode = GeometryInstance3D.GI_MODE_DISABLED
			instance.visible = false
		elif String(instance.name).ends_with("_detail"):
			instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			instance.gi_mode = GeometryInstance3D.GI_MODE_DYNAMIC
			instance.visibility_range_end = DETAIL_RANGE
			instance.visibility_range_end_margin = 4.0
		else:
			instance.gi_mode = GeometryInstance3D.GI_MODE_STATIC
	return scene


func _material(key: String) -> Material:
	for folder: String in ["res://materials/slamcity/", "res://materials/world/"]:
		var path: String = folder + key + ".tres"
		if ResourceLoader.exists(path):
			return load(path) as Material
	push_warning("slamcity_import: no material named %s" % key)
	return null
