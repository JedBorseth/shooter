extends Node3D
## Slamcity: an original city block at dusk round Slam Junction, a station on
## an elevated railway. Each team's half is the other's turned half round the
## centre, dressed as different places, so the West and East spawns face the
## same routes (tools/slamcity_layout.py).

## The train on the elevated line behind the north skyline: metres a second,
## and how far it runs before it comes round again.
const FAR_TRAIN_SPEED := 22.0
const FAR_TRAIN_RUN := 420.0

var _respawning := false
var _far_train: Node3D
var _far_train_x := 0.0


func _ready() -> void:
	_use_shadow_proxies()
	_far_train = get_node_or_null("City/far_train") as Node3D
	if _far_train != null:
		_far_train_x = _far_train.position.x


func _process(_delta: float) -> void:
	if _far_train == null:
		return
	# Westbound, out of sight beyond the skyline for most of its loop.
	var travelled := fmod(Time.get_ticks_msec() / 1000.0 * FAR_TRAIN_SPEED, FAR_TRAIN_RUN)
	_far_train.position.x = _far_train_x + FAR_TRAIN_RUN * 0.5 - travelled


## The city's meshes (tools/build_slamcity_city.py) come with a shadow-only
## twin per chunk: all its opaque geometry in one surface. In the game the twins
## cast the shadows and the real meshes do not, which costs one draw per chunk
## and shadow cascade instead of one per material. The editor keeps the real
## meshes casting, so the lightmap bake sees them.
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
	# The boundary walls should keep everyone in. If someone still escapes the
	# playable space offline, recover them instead of leaving them falling
	# forever. Online, the server respawns players.
	var player := get_node_or_null("Player") as Node3D
	if player == null or _respawning or Net.is_online():
		return
	if player.position.y < -20.0:
		_respawning = true
		get_tree().reload_current_scene.call_deferred()
