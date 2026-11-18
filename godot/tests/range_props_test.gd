extends SceneTree
## The Testing Area's range props: the hung steel plates swing when a round
## strikes their face (and only then), come back to rest, and stop working
## once at rest; the wall rack stands a gun in each slot, butt on the shelf.

var _failures: Array[String] = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	# The rack skips its guns on a dedicated server (headless); tests opt in.
	Engine.set_meta(&"wall_rack_headless", true)
	var scene: Node = (load("res://scenes/main.tscn") as PackedScene).instantiate()
	root.add_child(scene)
	current_scene = scene
	for i in 5:
		await process_frame
	await _check_plates(scene)
	_check_rack(scene)
	Engine.remove_meta(&"wall_rack_headless")
	scene.queue_free()
	await process_frame
	for failure in _failures:
		printerr("FAIL: ", failure)
	quit(1 if not _failures.is_empty() else 0)


func _check_plates(scene: Node) -> void:
	var plates := scene.get_node("SteelPlates").get_children()
	_check(plates.size() == 4, "four steel plates hang in the impact area")
	var plate := plates[0] as Node3D
	_check(plate.is_in_group(&"struck"), "plates hear about impacts")
	_check(not plate.is_processing(), "a plate at rest does no work")
	# A round on the wall a metre away: no swing.
	var normal := plate.global_basis.z.normalized()
	call_group(&"struck", &"struck", plate.global_position + Vector3(0, -0.5, 1.5), normal)
	_check(not plate.is_processing(), "a round that misses leaves the plate still")
	# A round on the face of the first plate.
	var face := plate.global_transform * Vector3(0.0, -float(plate.get("centre_drop")), 0.01)
	call_group(&"struck", &"struck", face, normal)
	for i in 8:
		await process_frame
	var swing: float = plate.call(&"swing")
	_check(absf(swing) > 0.05, "a round on the face swings the plate (%.3f rad)" % swing)
	_check(
		not (plates[1] as Node3D).is_processing() or absf(plates[1].call(&"swing")) < 0.001,
		"the plate beside it stays still"
	)
	var model := plate.get_node("Model") as Node3D
	_check(is_equal_approx(model.rotation.x, swing), "the model swings about the hinge")
	# It comes back to rest within a few seconds (about six), and stops
	# working there. Tests step one frame per physics tick.
	for i in Engine.physics_ticks_per_second * 10:
		if not plate.is_processing():
			break
		await process_frame
	_check(not plate.is_processing(), "the plate comes to rest and stops working")
	_check(is_zero_approx(model.rotation.x), "at rest it hangs straight")


func _check_rack(scene: Node) -> void:
	var rack := scene.get_node("WallRack") as Node3D
	var guns: Array[Node3D] = []
	for child in rack.get_children():
		if child.has_meta(&"rack_gun"):
			guns.append(child as Node3D)
	_check(guns.size() == 4, "the wall rack holds four guns (%d)" % guns.size())
	var shelf: float = (rack.get_script() as Script).get_script_constant_map()["SHELF"]
	for gun in guns:
		var box: AABB = rack.call(&"_bounds", gun, gun.transform)
		_check(
			absf(box.position.y - shelf) < 0.01,
			"%s stands on the shelf (butt at %.3f m)" % [gun.name, box.position.y]
		)
		_check(box.size.y > box.size.x and box.size.y > box.size.z, "%s stands upright" % gun.name)
		_check(
			box.position.x > -0.85 and box.end.x < 0.85 and box.position.z > 0.0,
			"%s is inside the rack, off the wall" % gun.name
		)
		_check(bool(gun.get(&"world_model")), "%s is the third-person model" % gun.name)


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)
