extends MultiMeshInstance3D
## Attach to Field grass/weed batches. Build batches in roughly 8 m cells,
## with transforms in shuffled order so a smaller prefix keeps even coverage.

const GraphicsQuality := preload("res://scripts/graphics.gd")
const LOW_DENSITY := 0.40
const LOW_RANGE := 24.0
const FULL_RANGE := 36.0
const FADE_MARGIN := 5.0


func _ready() -> void:
	if multimesh == null:
		return
	cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
	visibility_range_fade_mode = GeometryInstance3D.VISIBILITY_RANGE_FADE_SELF
	visibility_range_end_margin = FADE_MARGIN
	var settings := get_node_or_null("/root/Settings")
	if settings != null:
		settings.connect(&"changed", _apply_preset.bind(settings))
		_apply_preset(settings)
	else:
		visibility_range_end = FULL_RANGE


func _apply_preset(settings: Node) -> void:
	var low: bool = settings.get("graphics_quality") == GraphicsQuality.Preset.LOW
	multimesh.visible_instance_count = ceili(multimesh.instance_count * LOW_DENSITY) if low else -1
	visibility_range_end = LOW_RANGE if low else FULL_RANGE
