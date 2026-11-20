extends Node
## Rendering help for a map built from Blender models (tools/build_models.py).
## Put one in the map's scene.
##
## Shadow twins: a map model comes with a `map_*_shadow` mesh holding all its
## opaque geometry in one surface. In the game the twin casts the sun's shadows
## and the real meshes do not, which costs one draw per model and shadow
## cascade instead of one per material. The editor keeps the real meshes
## casting, so the lightmap bake sees them.
##
## On the Low preset the set dressing's small
## detail (`map_*_detail` meshes: litter, brass, weeds) is hidden, so the
## dressing costs Low nothing over the bare map. A sky shader with a `detailed`
## switch (shaders/night_sky.gdshader) drops its cloud, stars, and skyline on
## Low, and one with a `fine_clouds` switch reads its cloud's finer noise on
## High only.

const GraphicsQuality := preload("res://scripts/graphics.gd")

## The map's sun or moon.
## Unused: the presets cap every sun's splits on Low (scripts/graphics.gd).
## Kept so the scenes that set it still load cleanly.
@export var sun_path: NodePath

var _details: Array[MeshInstance3D] = []
var _settings: Node
var _sky: ShaderMaterial


func _ready() -> void:
	var scene := owner if owner != null else get_parent()
	for node in scene.find_children("map_*", "MeshInstance3D", true, false):
		var mesh := node as MeshInstance3D
		if mesh.name.ends_with("_shadow"):
			mesh.visible = true
		elif mesh.name.ends_with("_detail"):
			_details.append(mesh)
		elif not mesh.name.ends_with("_ground") and _has_twin(mesh):
			mesh.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	for env_node in scene.find_children("*", "WorldEnvironment", true, false):
		var env := (env_node as WorldEnvironment).environment
		if env != null and env.sky != null and env.sky.sky_material is ShaderMaterial:
			var material := env.sky.sky_material as ShaderMaterial
			if material.shader != null and material.shader.code.contains("uniform bool detailed"):
				_sky = material
	_settings = get_node_or_null(^"/root/Settings")
	if _settings != null:
		_settings.connect(&"changed", _apply_preset)
		_apply_preset()


func _apply_preset() -> void:
	var low: bool = int(_settings.get(&"graphics_quality")) == GraphicsQuality.Preset.LOW
	for mesh in _details:
		mesh.visible = not low
	if _sky != null:
		_sky.set_shader_parameter(&"detailed", not low)
		var high: bool = int(_settings.get(&"graphics_quality")) == GraphicsQuality.Preset.HIGH
		_sky.set_shader_parameter(&"fine_clouds", high)


## Whether the mesh's model has a shadow twin beside it.
func _has_twin(mesh: MeshInstance3D) -> bool:
	for sibling in mesh.get_parent().get_children():
		if sibling is MeshInstance3D and sibling.name.ends_with("_shadow"):
			return true
	return false
