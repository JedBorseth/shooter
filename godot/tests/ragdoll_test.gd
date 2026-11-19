extends SceneTree
## Headless ragdoll test: soldiers killed on a flat floor fall with the shot's
## push (a chest shot from behind, a headshot from the side, a shotgun blast,
## a crouched death), buckle at the knees instead of bowing back stiff when
## shot from the front, stay above the floor, come to rest and freeze, drop their
## gun, keep at most six ragdolls moving at once, and stand up whole again when
## they respawn. A remote player's body stays where it fell when they
## respawn. A knife kill crumples where it stands, and our own knife drops a
## remote player at once, undone if the server never confirms it. Exits
## non-zero on failure. Run from the repo root:
##   godot --headless --path godot --fixed-fps 120 --script res://tests/ragdoll_test.gd

const DUMMY_SCENE := preload("res://scenes/props/target_dummy.tscn")
const REMOTE_SCENE := preload("res://scenes/remote_player.tscn")
## Bones checked against the floor.
const BONES := [
	"Hips", "Head", "LeftHand", "RightHand", "LeftFoot", "RightFoot", "LeftLowerArm", "Chest"
]

var _failures: PackedStringArray = []
var _world: Node3D


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	_world = Node3D.new()
	root.add_child(_world)
	var floor_body := StaticBody3D.new()
	floor_body.collision_layer = 1
	var shape := CollisionShape3D.new()
	var box := BoxShape3D.new()
	box.size = Vector3(80, 1, 80)
	shape.shape = box
	shape.position = Vector3(0, -0.5, 0)
	floor_body.add_child(shape)
	_world.add_child(floor_body)
	await _sixtieths(5)

	await _chest_shot_from_behind()
	await _front_shot_buckles()
	await _headshot_from_the_side()
	await _shotgun_pushes_harder()
	await _crouched_death()
	await _cap()
	await _respawn()
	await _remote_leaves_body()
	await _knife_crumples()
	await _knife_kill_predicted()
	_finish()


func _dummy(at: Vector3, yaw := 0.0, crouching := false) -> TargetDummy:
	var dummy := DUMMY_SCENE.instantiate() as TargetDummy
	dummy.respawn_delay = 1000.0
	dummy.crouching = crouching
	_world.add_child(dummy)
	dummy.global_position = at
	dummy.rotation.y = yaw
	return dummy


func _kill(dummy: TargetDummy, travel: Vector3, part: int, force: float) -> void:
	dummy.apply_damage(1000.0, part == 0)
	var point := dummy.global_position + Vector3.UP * (1.65 if part == 0 else 1.3)
	dummy.react_to_hit(travel, true, point, part, force)


func _rig(dummy: TargetDummy) -> SoldierRig:
	return dummy.get_node("Soldier") as SoldierRig


func _lowest_bone(rig: SoldierRig) -> float:
	var lowest := INF
	for bone: String in BONES:
		lowest = minf(lowest, rig.bone_global_transform(bone).origin.y)
	return lowest


func _chest_shot_from_behind() -> void:
	# The dummy faces -Z; the shot comes from +Z travelling -Z, into its back.
	var dummy := _dummy(Vector3(0, 0, 0))
	await _sixtieths(10)
	var rig := _rig(dummy)
	var standing := rig.bone_global_transform("Hips").origin.y
	_check(standing > 0.8, "a standing soldier's hips are up, got %.2f" % standing)
	_kill(dummy, Vector3(0, 0, -1), 2, 3.0)
	await _sixtieths(6)
	_check(rig.is_ragdoll(), "a kill turns the body into a ragdoll")
	var lowest := INF
	for i in 90:
		await _sixtieths(1)
		lowest = minf(lowest, _lowest_bone(rig))
	var hips := rig.bone_global_transform("Hips").origin
	_check(hips.y < 0.45, "the body ends up on the ground, hips at %.2f" % hips.y)
	_check(lowest > -0.08, "no bone sinks through the floor, lowest %.2f" % lowest)
	var fall := rig.fall_direction()
	_check(fall.z < -0.6, "the body falls with the shot (toward -Z), toward %s" % fall)
	var yard := root.get_tree().get_first_node_in_group("corpse_yard") as CorpseYard
	_check(yard != null and yard.weapon_count() == 1, "the gun drops")
	if yard != null and yard.weapon_count() > 0:
		var gun := yard.find_child("DroppedWeapon", true, false) as Node3D
		_check(
			gun != null and gun.global_position.y < 0.5 and gun.global_position.y > -0.1,
			"the gun lands on the floor, at %s" % (gun.global_position if gun else Vector3.ZERO)
		)
	for i in 6 * 60:
		if rig.is_still():
			break
		await _sixtieths(1)
	_check(rig.is_still(), "the body comes to rest and freezes")
	var rested := rig.bone_global_transform("Head").origin
	await _sixtieths(30)
	_check(
		rig.bone_global_transform("Head").origin.distance_to(rested) < 0.001,
		"a frozen body stays put"
	)
	dummy.queue_free()


## Shot in the chest or head from the front, a body gives at the knees and
## folds over: its torso leans back less than 12 degrees while the hips are up,
## and the hips are well down within 0.3 s. Before, it toppled back as a stiff
## plank, leaning back 30 to 65 degrees.
func _front_shot_buckles() -> void:
	for part in [2, 0]:
		var dummy := _dummy(Vector3(-6, 0, 14))
		await _sixtieths(10)
		var rig := _rig(dummy)
		var standing := rig.bone_global_transform("Hips").origin.y
		# The dummy faces -Z; travelling +Z, the round strikes its front.
		_kill(dummy, Vector3(0, 0, 1), part, 3.0)
		var lean := 0.0
		var drop := 0.0
		for i in 42:
			await _sixtieths(1)
			var hips := rig.bone_global_transform("Hips").origin
			var torso := rig.bone_global_transform("Neck").origin - hips
			if hips.y > 0.55 and torso.length() > 0.01:
				# Leaning back is toward +Z, away from the way it faces.
				lean = maxf(lean, rad_to_deg(atan2(torso.z, torso.y)))
			if i == 17:
				drop = standing - hips.y
		_check(
			lean < 12.0, "shot from the front (part %d), it leans back only %.1f°" % [part, lean]
		)
		_check(drop > 0.3, "and its knees give, hips %.2f m lower at 0.3 s" % drop)
		dummy.queue_free()
		await _sixtieths(2)


func _headshot_from_the_side() -> void:
	var dummy := _dummy(Vector3(6, 0, 0))
	await _sixtieths(10)
	var rig := _rig(dummy)
	# Travelling +X: from the dummy's left side.
	_kill(dummy, Vector3(1, 0, 0), 0, 3.0)
	await _sixtieths(90)
	var fall := rig.fall_direction()
	_check(fall.x > 0.3, "a headshot from the side drops it away from the shot, toward %s" % fall)
	dummy.queue_free()


func _shotgun_pushes_harder() -> void:
	var rifle := _dummy(Vector3(-6, 0, 6))
	var shotgun := _dummy(Vector3(6, 0, 6))
	await _sixtieths(10)
	_kill(rifle, Vector3(0, 0, -1), 2, 3.0)
	_kill(shotgun, Vector3(0, 0, -1), 2, 15.0)
	await _sixtieths(45)
	var a := _rig(rifle).fall_distance()
	var b := _rig(shotgun).fall_distance()
	_check(b > a * 1.3, "a shotgun blast throws the body further (%.2f m vs %.2f m)" % [b, a])
	rifle.queue_free()
	shotgun.queue_free()


func _crouched_death() -> void:
	var dummy := _dummy(Vector3(-6, 0, -6), 0.0, true)
	await _sixtieths(20)
	var rig := _rig(dummy)
	var start := rig.bone_global_transform("Hips").origin.y
	_kill(dummy, Vector3(0, 0, -1), 2, 3.0)
	var highest := 0.0
	for i in 60:
		await _sixtieths(1)
		highest = maxf(highest, rig.bone_global_transform("Hips").origin.y)
	_check(start < 0.8, "a crouched soldier's hips are low, got %.2f" % start)
	_check(
		highest < start + 0.12,
		(
			"a crouched death falls from the crouch, not from standing (%.2f from %.2f)"
			% [highest, start]
		)
	)
	dummy.queue_free()


func _cap() -> void:
	var dummies: Array[TargetDummy] = []
	for i in 8:
		dummies.append(_dummy(Vector3(-12 + i * 3, 0, -14)))
	await _sixtieths(10)
	for dummy in dummies:
		_kill(dummy, Vector3(0, 0, -1), 2, 3.0)
	var most := 0
	for i in 60:
		await _sixtieths(1)
		most = maxi(most, CorpseYard.simulating())
	_check(most <= 6, "at most six ragdolls move at once, saw %d" % most)
	_check(_rig(dummies[0]).is_still(), "the oldest ragdoll freezes to make room")
	for dummy in dummies:
		dummy.queue_free()
	await _sixtieths(5)


func _respawn() -> void:
	var dummy := _dummy(Vector3(0, 0, 12))
	dummy.respawn_delay = 1.5
	await _sixtieths(10)
	var rig := _rig(dummy)
	_kill(dummy, Vector3(0, 0, -1), 2, 3.0)
	await _sixtieths(60)
	_check(rig.is_ragdoll() or rig.is_still(), "the dummy is down")
	await _sixtieths(90)
	await _sixtieths(20)
	_check(dummy.health() > 0.0, "the dummy respawns")
	var hips := rig.bone_global_transform("Hips").origin
	_check(not rig.is_ragdoll() and not rig.is_still(), "the ragdoll is gone after a respawn")
	_check(hips.y > 0.8, "it stands up whole, hips at %.2f" % hips.y)
	_check(rig.weapon_node() != null, "it holds a new gun")
	dummy.queue_free()


func _remote_leaves_body() -> void:
	var remote := REMOTE_SCENE.instantiate() as RemotePlayer
	_world.add_child(remote)
	var feet := Vector3(10, 0, 12)
	for i in 20:
		remote.apply_test_state({"feet": feet}, 1.0 / 60.0)
		await _sixtieths(1)
	var body := remote.soldier()
	var yard := root.get_tree().get_first_node_in_group("corpse_yard") as CorpseYard
	var lying := yard.corpse_count()
	for i in 30:
		remote.apply_test_state({"feet": feet, "alive": false}, 1.0 / 60.0)
		await _sixtieths(1)
	_check(yard.corpse_count() == lying + 1, "a remote player's body stays in the corpse yard")
	_check(body.get_parent() == yard, "the body that died is the yard's")
	_check(body.is_ragdoll() or body.is_still(), "it falls as a ragdoll")
	var fresh := remote.soldier()
	_check(fresh != body and not fresh.visible, "a fresh body waits hidden while dead")
	var spawn := Vector3(20, 0, 12)
	for i in 10:
		remote.apply_test_state({"feet": spawn}, 1.0 / 60.0)
		await _sixtieths(1)
	_check(fresh.visible, "the fresh body shows on respawn")
	_check(fresh.bone_global_transform("Hips").origin.y > 0.8, "the respawned player stands")
	_check(body.get_parent() == yard, "the old body still lies where it fell")
	remote.queue_free()


## A knife kill (force level 0, `HitPart::KNIFE_FORCE`) folds the body where
## it stands; a rifle kill with the same push topples it stiff-legged first.
func _knife_crumples() -> void:
	var knifed := _dummy(Vector3(-10, 0, -10))
	var shot := _dummy(Vector3(-14, 0, -10))
	await _sixtieths(10)
	var standing := _rig(knifed).bone_global_transform("Hips").origin.y
	_kill(knifed, Vector3(0, 0, 1), 2, 0.0)
	_kill(shot, Vector3(0, 0, 1), 2, 4.0)
	# A shot body buckles too, but the knife's legs give at once: it is
	# further down a tenth of a second in.
	await _sixtieths(6)
	var knife_drop := standing - _rig(knifed).bone_global_transform("Hips").origin.y
	var shot_drop := standing - _rig(shot).bone_global_transform("Hips").origin.y
	await _sixtieths(12)
	var knife_later := standing - _rig(knifed).bone_global_transform("Hips").origin.y
	_check(
		knife_drop > 0.07 and knife_drop > shot_drop * 1.4 and knife_later > 0.4,
		(
			"a knifed body is going down within 0.1 s, hips %.2f m lower (a shot one %.2f)"
			% [knife_drop, shot_drop]
		)
	)
	for i in 3 * 60:
		await _sixtieths(1)
	var hips := _rig(knifed).bone_global_transform("Hips").origin
	_check(hips.y < 0.45, "and ends up on the ground, hips at %.2f" % hips.y)
	_check(_lowest_bone(_rig(knifed)) > -0.08, "with no bone through the floor")
	knifed.queue_free()
	shot.queue_free()


## Our own knife strike drops a remote player at once, ahead of the server.
## A snapshot that confirms the death keeps that one body; if none confirms
## it within a second, the body goes and the player stands again.
func _knife_kill_predicted() -> void:
	var remote := REMOTE_SCENE.instantiate() as RemotePlayer
	_world.add_child(remote)
	var feet := Vector3(30, 0, 12)
	for i in 20:
		remote.apply_test_state({"feet": feet}, 1.0 / 60.0)
		await _sixtieths(1)
	var yard := root.get_tree().get_first_node_in_group("corpse_yard") as CorpseYard
	var lying := yard.corpse_count()
	var body := remote.soldier()
	_check(remote.apply_melee(150.0), "a knife strike predicts a kill")
	remote.react_to_hit(Vector3(0, 0, 1), true, Vector3.ZERO, 2, 0.0)
	_check(yard.corpse_count() == lying + 1, "the body falls on the strike")
	_check(not remote.is_alive(), "and the player is dead on this client")
	# The server has not answered yet: snapshots still show them alive.
	for i in 30:
		remote.apply_test_state({"feet": feet}, 1.0 / 60.0)
		await _sixtieths(1)
	_check(not remote.soldier().visible, "it stays down while the server's word is on its way")
	# It never comes: the server's strike missed.
	for i in 40:
		remote.apply_test_state({"feet": feet}, 1.0 / 60.0)
		await _sixtieths(1)
	await _sixtieths(1)
	_check(yard.corpse_count() == lying, "unconfirmed, the body goes")
	_check(not is_instance_valid(body), "and is freed")
	_check(remote.soldier().visible and remote.is_alive(), "and the player stands again")
	# Again, and this time the server confirms it: one body, the one that fell.
	body = remote.soldier()
	remote.apply_melee(150.0)
	remote.react_to_hit(Vector3(0, 0, 1), true, Vector3.ZERO, 2, 0.0)
	for i in 6:
		remote.apply_test_state({"feet": feet}, 1.0 / 60.0)
		await _sixtieths(1)
	for i in 80:
		remote.apply_test_state({"feet": feet, "alive": false}, 1.0 / 60.0)
		await _sixtieths(1)
	_check(yard.corpse_count() == lying + 1, "confirmed, one body lies there")
	_check(is_instance_valid(body) and body.get_parent() == yard, "the one that fell")
	remote.queue_free()


## Waits `count` sixtieths of a second of physics time, whatever the tick rate.
func _sixtieths(count: int) -> void:
	for i in count * Engine.physics_ticks_per_second / 60:
		await physics_frame


func _check(condition: bool, label: String) -> void:
	print("%s %s" % ["PASS" if condition else "FAIL", label])
	if not condition:
		_failures.append(label)


func _finish() -> void:
	print("ragdoll test: %d failure(s)" % _failures.size())
	quit(1 if _failures.size() > 0 else 0)
