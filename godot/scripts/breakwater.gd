extends Node3D
## Breakwater: an original coastal fuel terminal in late-morning sun, on a mole
## between two harbour basins, with a truck loading rack in the middle. Each
## team's half is the other's turned half round the centre, dressed as
## different places, so the West and East gates face the same routes
## (tools/breakwater_layout.py).

var _respawning := false


func _ready() -> void:
	_use_shadow_proxies()


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
