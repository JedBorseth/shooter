extends Node3D
## Warehouse: a corrugated-box plant rebuilt from reference photos
## (docs/map-design.md). The scene carries its own team starts and respawn
## points under SpawnPoints, so the dedicated server needs no preparation.

var _respawning := false


## The dedicated server calls this before it reads the spawn markers. The
## markers are authored in the scene, so there is nothing to build.
func prepare_server() -> void:
	pass


func _ready() -> void:
	# Attach Warehouse's area-aware Soundscape if the scene does not carry one yet.
	# Soundscape stays idle until a camera exists, so dedicated servers play nothing.
	if not has_node("Soundscape"):
		var scape := Soundscape.new()
		scape.name = "Soundscape"
		scape.preset = "warehouse"
		add_child(scape)
	# Repeated stock differs load to load; a dedicated server draws nothing.
	if DisplayServer.get_name() != "headless":
		PropVariation.apply([$PhotoPass/Stock, $PhotoPass/Machines] as Array[Node])
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


func _physics_process(_delta: float) -> void:
	# The yard boundary keeps players in. Should anyone still get off the map,
	# recover instead of leaving them stranded below it.
	if not Net.is_online() and not _respawning and $Player.position.y < -20.0:
		_respawning = true
		get_tree().reload_current_scene.call_deferred()
