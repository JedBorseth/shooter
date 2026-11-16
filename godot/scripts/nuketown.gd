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


func _ready() -> void:
	$HUD/ReturnToMenu.pressed.connect(
		func() -> void: get_tree().change_scene_to_file("res://scenes/title_screen.tscn")
	)
	$SecretLaser.place($Player.position, _spawn_yaw)
	$SecretLaser.laser_taken.connect(_spawn_laser_targets, CONNECT_ONE_SHOT)


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel"):
		Input.mouse_mode = Input.MOUSE_MODE_VISIBLE


func _physics_process(_delta: float) -> void:
	# Render geometry has no original invisible map-boundary brushes. Recover if
	# someone walks off the scenery instead of leaving them stranded below it.
	if not _respawning and $Player.position.y < -20.0:
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
