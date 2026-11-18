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
## On the Low preset the map's sun draws at most two shadow splits instead of
## its own count (Low's shadows reach only half as far), and the set dressing's small
## detail (`map_*_detail` meshes: litter, brass, weeds) is hidden, so the
## dressing costs Low nothing over the bare map. A sky shader with a `detailed`
## switch (shaders/night_sky.gdshader) drops its cloud and stars.

const GraphicsQuality := preload("res://scripts/graphics.gd")

## The map's sun or moon.
@export var sun_path: NodePath

var _sun: DirectionalLight3D
var _sun_mode := DirectionalLight3D.SHADOW_PARALLEL_4_SPLITS
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
	_sun = get_node_or_null(sun_path) as DirectionalLight3D
	if _sun != null:
		_sun_mode = _sun.directional_shadow_mode
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
	if _sun != null:
		_sun.directional_shadow_mode = (
			mini(_sun_mode, DirectionalLight3D.SHADOW_PARALLEL_2_SPLITS) if low else _sun_mode
		)
	for mesh in _details:
		mesh.visible = not low
	if _sky != null:
		_sky.set_shader_parameter(&"detailed", not low)


## Whether the mesh's model has a shadow twin beside it.
func _has_twin(mesh: MeshInstance3D) -> bool:
	for sibling in mesh.get_parent().get_children():
		if sibling is MeshInstance3D and sibling.name.ends_with("_shadow"):
			return true
	return false
