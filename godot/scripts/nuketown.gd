extends Node3D
## The converted world is a lazily loaded scene; startup stays UI-only.

var _respawning := false


func _enter_tree() -> void:
	var file := FileAccess.open("res://assets/maps/nuketown/generated/map.json", FileAccess.READ)
	if file == null:
		push_error("Nuketown map metadata is missing.")
		return
	var metadata: Dictionary = JSON.parse_string(file.get_as_text())
	var spawn: Dictionary = metadata["spawns"][0]
	var point: Array = spawn["position"]
	$Player.position = Vector3(float(point[0]), float(point[1]), float(point[2]))
	$Player.rotation.y = float(spawn["yaw"])


func _ready() -> void:
	$HUD/ReturnToMenu.pressed.connect(
		func() -> void: get_tree().change_scene_to_file("res://scenes/title_screen.tscn")
	)


func _unhandled_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_cancel"):
		Input.mouse_mode = Input.MOUSE_MODE_VISIBLE


func _physics_process(_delta: float) -> void:
	# Render geometry has no original invisible map-boundary brushes. Recover if
	# someone walks off the scenery instead of leaving them stranded below it.
	if not _respawning and $Player.position.y < -20.0:
		_respawning = true
		get_tree().reload_current_scene.call_deferred()
