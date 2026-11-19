extends SceneTree
## Headless locomotion test for the third-person soldier: standing still, the
## aim turns the upper body while the hips stay on the planted feet, until the
## aim is more than 60° off and the feet step round. A slide gets down as fast
## as the hit zones do, a landing flexes the knees without crouching, and the
## torso leans into a start and back from a stop. Standing on the edge of a
## step, each foot rests on the ground under it. A weapon switch lowers the old
## weapon before the new one comes up. A reload takes the magazine out of the
## gun with the support hand and seats it again; an empty one drops the old
## magazine, which falls and lies on the ground. Exits non-zero on failure.
## Run from the repo root:
##   godot --headless --path godot --fixed-fps 120 --script res://tests/locomotion_test.gd

const SOLDIER := preload("res://scenes/characters/soldier.tscn")
const DT := 1.0 / 120.0

var _failures: PackedStringArray = []


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	await _turn_in_place()
	await _slide_and_landing()
	await _lean()
	await _feet_on_a_step()
	await _switch()
	await _magazine()
	await _dropped_magazines()
	_finish()


## Where a bone faces (yaw, radians) in the drawn pose, after driving `state`.
func _facing(rig: SoldierRig, bone: String, state: Dictionary) -> float:
	rig.capture_pose()
	rig.drive(state, DT)
	await process_frame
	await process_frame
	# The skeleton faces +Z; turn that into the game's yaw (0 faces -Z).
	var forward := rig.posed_bone_transform(bone).basis.z
	return atan2(-forward.x, -forward.z) + PI


func _wrapped(a: float) -> float:
	return wrapf(a, -PI, PI)


func _turn_in_place() -> void:
	var rig := SOLDIER.instantiate() as SoldierRig
	root.add_child(rig)
	var state := {"weapon": "m4", "yaw": 0.0}
	for i in 40:
		rig.drive(state, DT)
		await process_frame
	var hips0 := await _facing(rig, "Hips", state)
	var chest0 := await _facing(rig, "UpperChest", state)
	# 45° left: the feet stay.
	state["yaw"] = deg_to_rad(45)
	for i in 30:
		rig.drive(state, DT)
		await process_frame
	var hips := _wrapped(await _facing(rig, "Hips", state) - hips0)
	var chest := _wrapped(await _facing(rig, "UpperChest", state) - chest0)
	_check(
		absf(hips) < deg_to_rad(5),
		"a 45° turn leaves the hips on the feet (%.1f°)" % rad_to_deg(hips)
	)
	_check(chest > deg_to_rad(35), "and turns the chest with the aim (%.1f°)" % rad_to_deg(chest))
	# 90° left: the feet step round within half a second.
	state["yaw"] = deg_to_rad(90)
	for i in 60:
		rig.drive(state, DT)
		await process_frame
	hips = _wrapped(await _facing(rig, "Hips", state) - hips0)
	_check(
		absf(hips - deg_to_rad(90)) < deg_to_rad(12),
		"a 90° turn steps the feet round (hips at %.1f°)" % rad_to_deg(hips)
	)
	rig.queue_free()


## The drawn bone's world position after driving `state` once.
func _posed(rig: SoldierRig, bone: String, state: Dictionary) -> Vector3:
	rig.capture_pose()
	rig.drive(state, DT)
	await process_frame
	await process_frame
	return rig.posed_bone_transform(bone).origin


func _slide_and_landing() -> void:
	var rig := SOLDIER.instantiate() as SoldierRig
	root.add_child(rig)
	var run := {"weapon": "m4", "velocity": Vector3(0, 0, -6.5), "sprinting": true}
	for i in 40:
		rig.drive(run, DT)
		await process_frame
	var standing := (await _posed(rig, "Hips", run)).y
	var slide := {"weapon": "m4", "velocity": Vector3(0, 0, -7.0), "stance": "sliding"}
	# Two drives per sample: 0.15 s is 18 ticks.
	for i in 8:
		await _posed(rig, "Hips", slide)
	var down := (await _posed(rig, "Hips", slide)).y
	_check(
		down < standing - 0.35,
		"a slide is down within 0.15 s (hips %.2f from %.2f)" % [down, standing]
	)
	# A fall of 0.6 s, then the landing.
	var air := {"weapon": "m4", "on_floor": false, "velocity": Vector3(0, -4, 0)}
	for i in 72:
		rig.drive(air, DT)
		await process_frame
	var still := {"weapon": "m4"}
	var lowest := INF
	for i in 30:
		lowest = minf(lowest, (await _posed(rig, "Hips", still)).y)
	var stood := standing
	for i in 60:
		rig.drive(still, DT)
		await process_frame
	stood = (await _posed(rig, "Hips", still)).y
	var dip := stood - lowest
	_check(dip > 0.03 and dip < 0.2, "a landing flexes the knees (%.3f m), no deep squat" % dip)
	rig.queue_free()


func _lean() -> void:
	var rig := SOLDIER.instantiate() as SoldierRig
	root.add_child(rig)
	var still := {"weapon": "m4"}
	for i in 40:
		rig.drive(still, DT)
		await process_frame
	var head0 := await _posed(rig, "Head", still)
	var hips0 := await _posed(rig, "Hips", still)
	# Off from standing toward -Z at 8 m/s².
	var most := 0.0
	for i in 12:
		var go := {"weapon": "m4", "velocity": Vector3(0, 0, -8.0 * (i + 1) * 2 * DT)}
		var head := await _posed(rig, "Head", go)
		var hips := await _posed(rig, "Hips", go)
		most = maxf(most, -((head - hips) - (head0 - hips0)).z)
	_check(most > 0.02, "the torso leans into a start (%.3f m forward)" % most)
	rig.queue_free()


func _feet_on_a_step() -> void:
	# An 18 cm step under the soldier's left side (the soldier faces -Z, so
	# its left is -X); the body stands on the step, as its capsule would.
	var step := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = Vector3(1.0, 0.18, 1.0)
	shape.shape = box
	step.add_child(shape)
	root.add_child(step)
	step.position = Vector3(-0.5, 0.09, 0)
	var ground := StaticBody3D.new()
	var ground_shape := CollisionShape3D.new()
	var slab := BoxShape3D.new()
	slab.size = Vector3(10, 1, 10)
	ground_shape.shape = slab
	ground_shape.position = Vector3(0, -0.5, 0)
	ground.add_child(ground_shape)
	root.add_child(ground)
	var camera := Camera3D.new()
	root.add_child(camera)
	camera.position = Vector3(3, 1.5, 3)
	camera.make_current()
	var rig := SOLDIER.instantiate() as SoldierRig
	root.add_child(rig)
	rig.position = Vector3(0, 0.18, 0)
	var still := {"weapon": "m4"}
	for i in 60:
		rig.drive(still, DT)
		await process_frame
	var left := await _posed(rig, "LeftFoot", still)
	var right := await _posed(rig, "RightFoot", still)
	# The animation's ankles stand about 8 cm over the soles.
	_check(left.y > 0.18 + 0.03, "the foot on the step stays on it (ankle %.3f)" % left.y)
	_check(
		right.y < left.y - 0.09,
		"the foot over the lower ground reaches down toward it (ankle %.3f)" % right.y
	)
	# Past the straight leg, the toe points down to the lower ground: the
	# ball of the foot (whose joint sits about 4 cm over the sole) reaches it.
	var toe := await _posed(rig, "RightToes", still)
	_check(toe.y < 0.07, "the toe reaches down to the lower ground (ball %.3f)" % toe.y)
	var hips := await _posed(rig, "Hips", still)
	_check(
		hips.y > 0.18 + 0.87 - 0.095,
		"the hips drop no more than 9 cm for it (%.3f)" % (hips.y - 0.18)
	)
	rig.queue_free()
	step.queue_free()
	ground.queue_free()
	camera.queue_free()


func _switch() -> void:
	var rig := SOLDIER.instantiate() as SoldierRig
	root.add_child(rig)
	for i in 30:
		rig.drive({"weapon": "m4"}, DT)
		await process_frame
	var rifle := rig.weapon_node()
	var low := rifle.global_position.y
	for i in 12:
		rig.drive({"weapon": "m1911"}, DT)
		await process_frame
	_check(rig.weapon_node() == rifle, "the old weapon is still in hand 0.1 s into a switch")
	_check(rifle.global_position.y < low - 0.05, "and lowering")
	for i in 30:
		rig.drive({"weapon": "m1911"}, DT)
		await process_frame
	_check(rig.weapon_node() != rifle, "the new weapon is out after 0.35 s")
	rig.queue_free()


## A rifle's magazine leaves the gun with the support hand (the gun swaps its
## one-piece `World` mesh for `WorldBody` and `WorldMagazine`) and is seated
## again; a pistol's drops away and a fresh one comes up from the belt.
func _magazine() -> void:
	for weapon in ["m4", "m1911"]:
		var rig := SOLDIER.instantiate() as SoldierRig
		root.add_child(rig)
		for i in 30:
			rig.drive({"weapon": weapon}, DT)
			await process_frame
		var mesh := rig.weapon_node().get_node("Model/Mesh") as Node3D
		var world := mesh.get_node("World") as Node3D
		var body := mesh.get_node("WorldBody") as Node3D
		var magazine := mesh.get_node("WorldMagazine") as Node3D
		var rest := magazine.position
		_check(world.visible and not body.visible, "%s: one mesh before the reload" % weapon)
		var furthest := 0.0
		var shown_away := false
		for i in 100:
			rig.drive({"weapon": weapon, "reload": i / 100.0}, DT)
			await process_frame
			if magazine.visible:
				furthest = maxf(furthest, magazine.position.distance_to(rest))
				shown_away = shown_away or magazine.position.distance_to(rest) > 0.1
		_check(shown_away, "%s: the magazine leaves the gun (%.2f m)" % [weapon, furthest])
		rig.drive({"weapon": weapon}, DT)
		await process_frame
		_check(world.visible and not body.visible, "%s: one mesh again after it" % weapon)
		rig.queue_free()


## An empty reload drops the old magazine to the ground, where it lies on its
## side; a tactical reload stows it; a pistol's always drops.
func _dropped_magazines() -> void:
	var ground := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var slab := BoxShape3D.new()
	slab.size = Vector3(10, 1, 10)
	shape.shape = slab
	shape.position = Vector3(0, -0.5, 20)
	ground.add_child(shape)
	root.add_child(ground)
	var drops := func() -> Node: return get_first_node_in_group("magazine_drops")
	var dropped := func() -> int:
		var node: Node = drops.call()
		return 0 if node == null else node.dropped()
	for run in [["m4", false, 0], ["m4", true, 1], ["ak47", true, 1], ["m1911", false, 1]]:
		var before: int = dropped.call()
		var rig := SOLDIER.instantiate() as SoldierRig
		root.add_child(rig)
		rig.position = Vector3(0, 0, 20)
		var state := {"weapon": run[0]}
		for i in 30:
			rig.drive(state, DT)
			await process_frame
		for i in 240:
			state = {"weapon": run[0], "reload": i / 240.0, "reload_empty": run[1]}
			rig.drive(state, DT)
			await process_frame
		for i in 120:
			rig.drive({"weapon": run[0]}, DT)
			await process_frame
		var count: int = dropped.call() - before
		_check(
			count == run[2],
			(
				"%s %s reload: drops %d magazine(s) (%d)"
				% [run[0], "empty" if run[1] else "tactical", run[2], count]
			)
		)
		rig.queue_free()
	var node: Node = drops.call()
	if node != null:
		_check(node.resting() == node.showing(), "the dropped magazines lie still")
		for low: Vector3 in node.lowest_points():
			_check(
				low.y > -0.01 and low.y < 0.02,
				"a dropped magazine lies on the ground, its lowest point at %.3f m" % low.y
			)
	ground.queue_free()


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("locomotion test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
