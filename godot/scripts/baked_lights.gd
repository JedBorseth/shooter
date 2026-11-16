extends Node3D
## Holds lights that exist only to be baked into the map's LightmapGI. The
## editor keeps them so tools/bake_lightmaps.sh can bake them; in the game they
## are removed, because their light is already in the lightmap and its probes,
## and drawing them live (with omni shadows) would cost frame time for nothing.

## Set on Engine by tools that load and re-save a map, so the lights survive.
const KEEP := &"keep_baked_lights"


func _ready() -> void:
	if not Engine.is_editor_hint() and not Engine.has_meta(KEEP):
		queue_free()
