extends SceneTree
## Headless zombie test: a zombie's head zone sits on its drawn, hunched head;
## a heavy hit staggers it in place while a light one does not; and one killed
## in the air falls to the floor as a ragdoll. Exits non-zero on failure.
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
	_finish()


func _zombie(at: Vector3) -> Zombie:
	var zombie := ZOMBIE.instantiate() as Zombie
	_world.add_child(zombie)
	zombie.global_position = at
	for i in 30:
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
	zombie.global_position = Vector3(-4, 1.5, 0)
	await physics_frame
	zombie.apply_damage(10000.0, false)
	zombie.react_to_hit(Vector3(0, 0, -1), true, Vector3.ZERO, 2, 3.0)
	for i in 240:
		await physics_frame
	var hips := rig.bone_global_transform("Hips").origin
	_check(hips.y < 0.5, "a zombie killed in the air falls to the floor (hips %.2f)" % hips.y)
	zombie.queue_free()


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("zombie reactions test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
