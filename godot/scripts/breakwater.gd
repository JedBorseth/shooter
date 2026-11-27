extends Node3D
## Breakwater: an original coastal fuel terminal in late-morning sun, on a mole
## between two harbour basins, with a truck loading rack in the middle. Each
## team's half is the other's turned half round the centre, dressed as
## different places, so the West and East gates face the same routes
## (tools/breakwater_layout.py).

## Props no longer than this (metres) cast no sun shadow in the game and hide
## beyond DETAIL_RANGE: drums, cans, downpipe lengths, wall boxes. Their
## shadows are in the lightmap; drawing them into every shadow cascade cost a
## draw call each.
const SMALL_PROP := 1.3
## Matches the map mesh's `_detail` chunks (tools/breakwater_import.gd).
const DETAIL_RANGE := 45.0

var _respawning := false


func _ready() -> void:
	_use_shadow_proxies()
	_quiet_small_props()


## The map's meshes (tools/build_breakwater_city.py) come with a shadow-only
## twin per chunk. In the game the twins cast the shadows and the real meshes
## do not; the editor keeps the real meshes casting, so the lightmap bake sees
## them.
func _use_shadow_proxies() -> void:
	var city := get_node_or_null("City")
	if city == null:
		return
	for node in city.find_children("*", "MeshInstance3D", true, false):
		var mesh := node as MeshInstance3D
		if mesh.name.ends_with("_shadow"):
			mesh.visible = true
		elif not mesh.name.ends_with("_detail"):
			mesh.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF


## The library props (tools/breakwater_layout.py `prop`): small ones cast no
## sun shadow and fade out at a distance, like the map's own detail. The
## editor leaves them alone, so the lightmap bake still sees their shadows.
func _quiet_small_props() -> void:
	var props := get_node_or_null("Props")
	if props == null:
		return
	for node in props.find_children("*", "MeshInstance3D", true, false):
		var mesh := node as MeshInstance3D
		var size := mesh.get_aabb().size * mesh.global_transform.basis.get_scale()
		if maxf(size.x, maxf(size.y, size.z)) < SMALL_PROP:
			mesh.cast_shadow = GeometryInstance3D.SHADOW_CASTING_SETTING_OFF
			mesh.visibility_range_end = DETAIL_RANGE
			mesh.visibility_range_end_margin = 4.0


## Called by the dedicated server before it removes the local player.
func prepare_server() -> void:
	set_physics_process(false)
	set_process(false)


func _physics_process(_delta: float) -> void:
	# Offline, recover a player who escapes the playable space; online the
	# server respawns players.
	var player := get_node_or_null("Player") as Node3D
	if player == null or _respawning or Net.is_online():
		return
	if player.position.y < -20.0:
		_respawning = true
		get_tree().reload_current_scene.call_deferred()
