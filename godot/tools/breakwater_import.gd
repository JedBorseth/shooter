@tool
extends EditorScenePostImport
## Import script for Breakwater's meshes (assets/maps/breakwater/city.gltf,
## written by tools/build_breakwater_city.py): each surface's placeholder
## material is swapped for the shared material of that name in
## materials/breakwater/ (or a look in materials/world/). `*_detail` chunks
## cast no shadow, are hidden beyond 45 m, and take light from the lightmap's
## probes; `*_shadow` chunks are shadow-only proxies (see scripts/breakwater.gd). The `far_*` chunks (the
## land, water and ships beyond the boundary, one per quadrant so each culls on its own) are too
## big for lightmap texels: they take the probes' light and cast no shadow.

const DETAIL_RANGE := 45.0
## Beyond the boundary, `far_*_detail` (rails, marks, small fittings on the
## ship and the far plant) draws this far.
const FAR_DETAIL_RANGE := 90.0


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
		var chunk := String(instance.name)
		if chunk.begins_with("far"):
			instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			instance.gi_mode = GeometryInstance3D.GI_MODE_DYNAMIC
			if chunk.ends_with("_detail"):
				instance.visibility_range_end = FAR_DETAIL_RANGE
				instance.visibility_range_end_margin = 10.0
		elif chunk.ends_with("_shadow"):
			instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_SHADOWS_ONLY
			instance.gi_mode = GeometryInstance3D.GI_MODE_DISABLED
			instance.visible = false
		elif chunk.ends_with("_stain"):
			# Stains laid over the ground: a multiply-blend over the lit ground
			# (shaders/stain_mul.gdshader), out of the bake and casting nothing.
			instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			instance.gi_mode = GeometryInstance3D.GI_MODE_DISABLED
		elif chunk.ends_with("_detail"):
			instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			instance.gi_mode = GeometryInstance3D.GI_MODE_DYNAMIC
			instance.visibility_range_end = DETAIL_RANGE
			instance.visibility_range_end_margin = 4.0
		else:
			instance.gi_mode = GeometryInstance3D.GI_MODE_STATIC
	return scene


func _material(key: String) -> Material:
	for folder: String in ["res://materials/breakwater/", "res://materials/world/"]:
		var path: String = folder + key + ".tres"
		if ResourceLoader.exists(path):
			return load(path) as Material
	push_warning("breakwater_import: no material named %s" % key)
	return null
