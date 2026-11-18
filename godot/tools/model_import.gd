@tool
extends EditorScenePostImport
## Import script for the standalone models in assets/models/ (written by
## tools/build_models.py). Each surface's material is a placeholder named after
## a shared material in materials/models/; this swaps the real one in. Most
## models are props that can move or vanish, so they take light from a map's
## lightmap probes rather than baking into it; a `map_*` model is a map's
## static architecture and bakes into its lightmap (import it with static
## lightmap baking, so it gets lightmap UVs). A map's set dressing
## (`map_*_dressing`) comes in chunks; its `_detail` chunks (litter, weeds,
## small fittings) cast no shadow, take light from the probes instead of the
## lightmap, and hide beyond `DETAIL_RANGE`; `_ground` meshes (a floor and what
## lies flat on it) bake but cast no shadow. A
## `_shadow` mesh is a shadow-only twin of a map model's other meshes (see
## scripts/map_render.gd).

## How far a dressing's detail meshes draw, in metres.
const DETAIL_RANGE := 30.0


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
		if String(instance.name).begins_with("map_") and String(instance.name).ends_with("_shadow"):
			# A shadow-only twin of the map's meshes (scripts/map_render.gd
			# swaps it in for them in the game). Hidden in the editor, so the
			# lightmap bake sees the real meshes.
			instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_SHADOWS_ONLY
			instance.gi_mode = GeometryInstance3D.GI_MODE_DISABLED
			instance.visible = false
		elif (
			String(instance.name).begins_with("map_") and String(instance.name).ends_with("_detail")
		):
			instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			instance.gi_mode = GeometryInstance3D.GI_MODE_DYNAMIC
			instance.visibility_range_end = DETAIL_RANGE
			instance.visibility_range_end_margin = 4.0
		elif String(instance.name).begins_with("map_"):
			instance.gi_mode = GeometryInstance3D.GI_MODE_STATIC
			if String(instance.name).ends_with("_ground"):
				# Floors and what lies flat on them shade nothing.
				instance.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
		else:
			instance.gi_mode = GeometryInstance3D.GI_MODE_DYNAMIC
	return scene


func _material(key: String) -> Material:
	var path := "res://materials/models/%s.tres" % key
	if ResourceLoader.exists(path):
		return load(path) as Material
	push_warning("model_import: no material named %s" % key)
	return null
