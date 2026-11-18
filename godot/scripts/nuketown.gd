extends Node3D
## The converted world is a lazily loaded scene; startup stays UI-only.

## Soldiers that appear at the far spawn points once someone finds the laser,
## so it has something to vaporize across the map.
const LASER_TARGETS := 8
const TARGET_SCENE := preload("res://scenes/props/target_dummy.tscn")

var _respawning := false
var _spawns: Array = []
var _spawn_yaw := 0.0


func _enter_tree() -> void:
	if has_node("Player"):
		prepare_server()


## Also called before the dedicated server reads and removes the spawn marker.
func prepare_server() -> void:
	var file := FileAccess.open("res://assets/maps/nuketown/generated/map.json", FileAccess.READ)
	if file == null:
		push_error("Nuketown map metadata is missing.")
		return
	var metadata: Dictionary = JSON.parse_string(file.get_as_text())
	_spawns = metadata["spawns"]
	var spawn: Dictionary = _spawns[0]
	var point: Array = spawn["position"]
	_spawn_yaw = float(spawn["yaw"])
	$Player.position = Vector3(float(point[0]), float(point[1]), float(point[2]))
	$Player.rotation.y = _spawn_yaw
	_prepare_spawn_points()


## Validated deathmatch points cover both yards, houses, and lanes. The six
## outermost ground points on each side are starts; all points allow respawns.
## Idempotent: both server and offline scene initialization call preparation.
func _prepare_spawn_points() -> void:
	if has_node("SpawnPoints"):
		return
	var parent := Node3D.new()
	parent.name = "SpawnPoints"
	add_child(parent)
	var ground: Array[Marker3D] = []
	var data: Dictionary = JSON.parse_string(
		FileAccess.get_file_as_string("res://data/nuketown_spawns.json")
	)
	for spawn: Dictionary in data["spawns"]:
		var point: Array = spawn["position"]
		var marker := Marker3D.new()
		marker.position = Vector3(float(point[0]), float(point[1]), float(point[2]))
		marker.rotation.y = float(spawn["yaw"])
		parent.add_child(marker)
		if marker.position.y < 1.0:
			ground.append(marker)
	ground.sort_custom(func(a: Marker3D, b: Marker3D) -> bool: return a.position.x < b.position.x)
	for slot in mini(6, ground.size() / 2):
		ground[slot].set_meta("team", 0)
		ground[ground.size() - 1 - slot].set_meta("team", 1)


func _ready() -> void:
	if not has_node("Player"):
		set_physics_process(false)
		return
	$HUD/ReturnToMenu.pressed.connect(
		func() -> void:
			if Net.is_online():
				Net.leave()
			else:
				get_tree().change_scene_to_file("res://scenes/title_screen.tscn")
	)
	$SecretLaser.place($Player.position, _spawn_yaw)
	$SecretLaser.laser_taken.connect(_spawn_laser_targets, CONNECT_ONE_SHOT)


func _physics_process(_delta: float) -> void:
	# The baked boundary keeps players in. Should anyone still get off the
	# scenery, recover instead of leaving them stranded below it.
	if not Net.is_online() and not _respawning and $Player.position.y < -20.0:
		_respawning = true
		get_tree().reload_current_scene.call_deferred()


## Puts target soldiers on the spawn points farthest from the player's spawn,
## each facing it.
func _spawn_laser_targets() -> void:
	if _spawns.is_empty():
		return
	var points: Array[Vector3] = []
	for spawn: Dictionary in _spawns:
		var point: Array = spawn["position"]
		points.append(Vector3(float(point[0]), float(point[1]), float(point[2])))
	var home := points[0]
	points.sort_custom(
		func(a: Vector3, b: Vector3) -> bool: return a.distance_to(home) > b.distance_to(home)
	)
	var targets := Node3D.new()
	targets.name = "LaserTargets"
	add_child(targets)
	for point: Vector3 in points.slice(0, LASER_TARGETS):
		var target: Node3D = TARGET_SCENE.instantiate()
		targets.add_child(target)
		target.global_position = point
		var to_home := home - point
		target.rotation.y = atan2(-to_home.x, -to_home.z)
