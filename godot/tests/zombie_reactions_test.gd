extends SceneTree
## Headless zombie test: a zombie's head zone sits on its drawn, hunched head;
## a heavy hit staggers it in place while a light one does not; and one killed
## in the air falls to the floor as a ragdoll; and bodies come to rest with
## no limb raised, though zombies die hunched. Exits non-zero on failure.
## Run from the repo root:
##   godot --headless --path godot --fixed-fps 120 --script res://tests/zombie_reactions_test.gd

const ZOMBIE := preload("res://scenes/zombies/zombie.tscn")
const DT := 1.0 / 120.0

var _failures: PackedStringArray = []
var _world: Node3D


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_world = Node3D.new()
	root.add_child(_world)
	var ground := StaticBody3D.new()
	var shape := CollisionShape3D.new()
	var slab := BoxShape3D.new()
	slab.size = Vector3(40, 1, 40)
	shape.shape = slab
	shape.position = Vector3(0, -0.5, 0)
	ground.add_child(shape)
	_world.add_child(ground)
	await _head_zone()
	await _stagger()
	await _mid_air_death()
	await _limbs_rest_down()
	_finish()


## A zombie standing at `at`, placed as the client places the server's.
func _zombie(at: Vector3) -> Zombie:
	var zombie := ZOMBIE.instantiate() as Zombie
	_world.add_child(zombie)
	for i in 30:
		zombie.net_apply(at, 0.0, Vector3.ZERO, false)
		await physics_frame
	return zombie


func _head_zone() -> void:
	var zombie := await _zombie(Vector3(0, 0, 0))
	var rig := zombie.get_node("Soldier") as SoldierRig
	rig.capture_pose()
	await process_frame
	await process_frame
	var head := rig.posed_bone_transform("Head").origin
	var shapes: Array = (zombie.get_node("Hitboxes") as HitboxRig).shapes()
	# The head sphere, then the neck from its joint to the head's.
	var neck_top: Vector3 = shapes[1][2]
	_check(
		neck_top.distance_to(head) < 0.1,
		"the head zone is on the drawn head (%.3f m off)" % neck_top.distance_to(head)
	)
	zombie.queue_free()


func _stagger() -> void:
	var zombie := await _zombie(Vector3(4, 0, 0))
	zombie.react_to_hit(Vector3(1, 0, 0), false, Vector3.ZERO, 2, 3.0)
	_check(zombie.staggered_for() == 0.0, "a rifle round does not stagger a zombie")
	zombie.react_to_hit(Vector3(1, 0, 0), false, Vector3.ZERO, 2, 12.0)
	_check(zombie.staggered_for() > 0.3, "a heavy hit staggers it")
	for i in 72:
		await physics_frame
	_check(zombie.staggered_for() == 0.0, "and it recovers within 0.6 s")
	zombie.queue_free()


func _mid_air_death() -> void:
	var zombie := await _zombie(Vector3(-4, 0, 0))
	var rig := zombie.get_node("Soldier") as SoldierRig
	zombie.net_apply(Vector3(-4, 1.5, 0), 0.0, Vector3.ZERO, false)
	await physics_frame
	zombie.net_die(Vector3(0, 0, -1), 2, 3.0)
	for i in 240:
		await physics_frame
	var hips := rig.bone_global_transform("Hips").origin
	_check(hips.y < 0.5, "a zombie killed in the air falls to the floor (hips %.2f)" % hips.y)
	zombie.queue_free()


## Zombies die hunched, arms reaching. Killed from each side, by a headshot,
## and by a shotgun, every body ends lying with no knee held up over its hip
## or hand over its shoulder: the joints settle about the limbs' natural
## rest, not about the pose each died in. They come to rest and stop on
## their own rather than at the time cap.
func _limbs_rest_down() -> void:
	var worst_knee := 0.0
	var worst_hand := 0.0
	var rested := 0
	# Travel, hit shape, force: chest hits from each side, a headshot, and a
	# shotgun blast.
	var hits := [
		[Vector3(0, 0, 1), 2, 6.0],
		[Vector3(0, 0, -1), 2, 6.0],
		[Vector3(1, 0, 0), 2, 6.0],
		[Vector3(-1, 0, 0), 2, 6.0],
		[Vector3(0, 0, 1), 0, 6.0],
		[Vector3(1, 0, 0), 2, 15.0],
	]
	for i in hits.size():
		var at := Vector3(-15 + i * 6, 0, 10)
		var zombie := await _zombie(at)
		var rig := zombie.get_node("Soldier") as SoldierRig
		zombie.net_die(hits[i][0], hits[i][1], hits[i][2])
		for t in 900:
			await physics_frame
			if t > 120 and rig.is_still():
				break
		var raised := _raised(rig)
		if rig.came_to_rest():
			rested += 1
		print(
			(
				"zombie %d: knee %.2f m over its hip, hand %.2f m over its shoulder, %s"
				% [
					i,
					raised.x,
					raised.y,
					"came to rest" if rig.came_to_rest() else "frozen by the time cap"
				]
			)
		)
		worst_knee = maxf(worst_knee, raised.x)
		worst_hand = maxf(worst_hand, raised.y)
		if is_instance_valid(zombie):
			zombie.queue_free()
	_check(worst_knee < 0.2, "no knee is left raised over its hip (%.2f m)" % worst_knee)
	_check(worst_hand < 0.2, "no hand is left raised over its shoulder (%.2f m)" % worst_hand)
	# Bodies settle and stop on their own, not frozen mid-move by the 5 s
	# cap (one slow topple may still run to it).
	_check(
		rested >= hits.size() - 1,
		"bodies come to rest before the time cap (%d of %d)" % [rested, hits.size()]
	)


## How far the highest knee sits above its own hip joint (`x`) and the
## highest hand above its own shoulder (`y`): a limb held up off a lying body.
func _raised(rig: SoldierRig) -> Vector2:
	var y := func(bone: String) -> float: return rig.bone_global_transform(bone).origin.y
	var knee := maxf(
		y.call("LeftLowerLeg") - y.call("LeftUpperLeg"),
		y.call("RightLowerLeg") - y.call("RightUpperLeg")
	)
	var hand := maxf(
		y.call("LeftHand") - y.call("LeftUpperArm"), y.call("RightHand") - y.call("RightUpperArm")
	)
	return Vector2(knee, hand)


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("zombie reactions test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
