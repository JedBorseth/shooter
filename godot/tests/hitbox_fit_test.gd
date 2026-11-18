extends SceneTree
## Headless test that the hit zones follow the animated soldier: "what you see
## is what you hit". A remote player is posed through every state another
## player can be seen in (idle, walking, running, sprinting, strafing, crouched,
## sliding, aiming, reloading, mantling, in the air, each weapon, looking up and
## down, turning), and for each hit zone it measures:
##
## - axis: how far the skeleton's bones are from the zone's capsule axes (the
##   larger of bone-to-axis and axis-to-bone, sampled along both), in cm;
## - cover: the share of the zone's visible skin (skinned mesh vertices,
##   assigned to a zone by their heaviest bone) that lies inside the zone's
##   shapes;
## - out: how far outside its zone's shapes the 90th-percentile vertex lies, cm.
##
## In every settled state (standing, crouched, sliding, aimed, mid-reload, in
## the air, each weapon, any pitch, turning) it fails when a zone's axis error
## passes AXIS_TOLERANCE or its out distance passes OUT_TOLERANCE. Where the hit
## zones cannot know how far an animation has run (a stride's swing, a reload's
## reach out and back, a mantle's climb) they hold the cycle's average or the
## middle, and the looser PHASE bounds guard against regressions. The hit zones
## are `shooter_sim::hitbox`, the same function the server rewinds; the body is
## the real SoldierRig and SoldierPoser.
## Run from the repo root:
##   godot --headless --path godot --script res://tests/hitbox_fit_test.gd
## It prints every state's numbers (cm, and cover in %); HITBOX_FIT_SHAPES=1
## also prints each shape's error and the segments it compared.
## HITBOX_FIT_DUMP=$PWD/rust/sim/src/hitbox/poses.rs (an absolute path, from
## the repo root) instead writes the base poses the hit zones start from,
## measured from the animations; do that when an animation changes, then run
## `cargo fmt` in rust/.

const REMOTE_SCENE := preload("res://scenes/remote_player.tscn")
const DT := 1.0 / 60.0
## Metres. In settled states every zone's bones stay this close to its capsule
## axes, and nine in ten of its skin's vertices this close to its shapes.
const AXIS_TOLERANCE := 0.06
const OUT_TOLERANCE := 0.05
## Axis bounds (m) per zone where the phase is unknown: mid-stride (the zones
## hold the stride's average), a reload's reach out and back (they hold the
## middle, the hand at the pouch), and a mantle's climb (its average).
const PHASE := {
	"stride": {"head": 0.13, "chest": 0.13, "stomach": 0.13, "arm": 0.23, "leg": 0.62},
	"reach": {"arm": 0.45},
	"climb": {"head": 0.35, "chest": 0.35, "stomach": 0.35, "arm": 0.35, "leg": 0.45},
}
## Skeleton points each base pose records, in body space.
const POSE_BONES := [
	"Hips",
	"Spine",
	"Chest",
	"UpperChest",
	"Neck",
	"Head",
	"LeftUpperArm",
	"RightUpperArm",
	"LeftUpperLeg",
	"LeftLowerLeg",
	"LeftFoot",
	"RightUpperLeg",
	"RightLowerLeg",
	"RightFoot",
]
const ZONES := ["head", "chest", "stomach", "arm", "leg"]
## Every vertex in VERTEX_STEP is skinned on the CPU.
const VERTEX_STEP := 3

var _failures: PackedStringArray = []
var _dump_path := OS.get_environment("HITBOX_FIT_DUMP")
## Bone poses (skeleton space) captured after the poser ran this frame.
var _poses: Array[Transform3D] = []
## Per state and zone: worst axis error, worst cover, worst out.
var _table: Array = []


func _initialize() -> void:
	_run.call_deferred()


func _states() -> Array[Dictionary]:
	var states: Array[Dictionary] = []
	var add := func(name: String, state: Dictionary, extra := {}) -> void:
		var entry := {"name": name, "state": state}
		entry.merge(extra)
		states.append(entry)
	add.call("idle", {})
	var stride := {"phase": "stride", "samples": [0, 7, 15, 22, 30, 37, 45, 52]}
	var moving := func(velocity: Vector3) -> Dictionary:
		var out := stride.duplicate()
		out["velocity"] = velocity
		return out
	add.call("walk", {}, moving.call(Vector3(0, 0, -1.5)))
	add.call("run", {}, moving.call(Vector3(0, 0, -4.0)))
	add.call("sprint", {"sprinting": true}, moving.call(Vector3(0, 0, -6.0)))
	add.call("strafe", {}, moving.call(Vector3(3.0, 0, 0)))
	add.call("strafe_run", {}, moving.call(Vector3(-2.8, 0, -2.8)))
	add.call("backpedal", {}, moving.call(Vector3(0, 0, 2.0)))
	add.call("crouch", {"stance": "crouching"})
	add.call("crouch_walk", {"stance": "crouching"}, moving.call(Vector3(0, 0, -1.2)))
	add.call("crouch_strafe", {"stance": "crouching"}, moving.call(Vector3(1.2, 0, 0)))
	add.call("slide", {"stance": "sliding"}, {"velocity": Vector3(0, 0, -7.0)})
	add.call("ads", {"ads": 1.0})
	add.call("ads_walk", {"ads": 1.0}, moving.call(Vector3(0, 0, -1.2)))
	add.call("crouch_ads", {"stance": "crouching", "ads": 1.0})
	# Reloads: frames into the reload for progress 0.3, 0.45, and 0.55 (the
	# hand at the pouch), then 0.05, 0.15, 0.7, and 0.85 (reaching out and back).
	for reload: Array in [
		["reload", {}, 126.0],
		["crouch_reload", {"stance": "crouching"}, 126.0],
		["m1911_reload", {"weapon": "m1911"}, 90.0],
		["m870_reload", {"weapon": "m870"}, 174.0],
	]:
		var state: Dictionary = reload[1]
		var frames := func(progress: Array) -> Array:
			return progress.map(func(p: float) -> int: return int(p * reload[2]))
		var then := {"reloading": true}
		add.call(reload[0], state, {"then": then, "samples": frames.call([0.3, 0.45, 0.55])})
		add.call(
			reload[0] + "_reach",
			state,
			{"then": then, "samples": frames.call([0.05, 0.15, 0.7, 0.85]), "phase": "reach"}
		)
	add.call("airborne", {"on_floor": false}, {"settle": 30, "samples": [0, 10, 20]})
	add.call("jump_run", {"on_floor": false}, {"settle": 30, "velocity": Vector3(0, 0, -4.0)})
	# A mantle up a metre takes about 25 frames.
	add.call(
		"mantle", {}, {"then": {"mantling": true}, "samples": [3, 9, 15, 21], "phase": "climb"}
	)
	for weapon in ["ak47", "br3", "lsw6", "m40", "pdw9", "m870", "m1911"]:
		add.call(weapon, {"weapon": weapon})
		add.call(weapon + "_ads", {"weapon": weapon, "ads": 1.0})
	for pitch_deg in [-89, -45, 45, 89]:
		var pitch := deg_to_rad(pitch_deg)
		add.call("pitch_%d" % pitch_deg, {"pitch": pitch})
		add.call("ads_pitch_%d" % pitch_deg, {"pitch": pitch, "ads": 1.0})
		add.call("crouch_pitch_%d" % pitch_deg, {"stance": "crouching", "pitch": pitch})
	add.call(
		"slide_pitch_60", {"stance": "sliding", "pitch": 1.05}, {"velocity": Vector3(0, 0, -7.0)}
	)
	add.call("turning", {}, {"yaw_rate": 3.0})
	add.call("turned", {"yaw": 2.2, "pitch": 0.4})
	return states


func _run() -> void:
	var world := Node3D.new()
	root.add_child(world)
	if not _dump_path.is_empty():
		await _generate(world)
		world.queue_free()
		quit(0)
		return
	_check_grips()
	for spec in _states():
		await _measure(world, spec)
	_report()
	world.queue_free()
	_finish()


## Each weapon file's `grips` (which place the arm hit zones' hands) match its
## scene's GripRight and GripLeft markers and grip rake (which place the drawn hands).
func _check_grips() -> void:
	var pattern := RegEx.create_from_string(
		"grips: Some\\(\\(right: \\(([^)]*)\\), left: \\(([^)]*)\\), rake: ([-0-9.]+)\\)\\)"
	)
	var parse := func(text: String) -> Vector3:
		var parts := text.split(",")
		return Vector3(float(parts[0]), float(parts[1]), float(parts[2]))
	for file in DirAccess.get_files_at("res://data/weapons"):
		var key := file.get_basename()
		var found := pattern.search(FileAccess.get_file_as_string("res://data/weapons/" + file))
		_check(found != null, "%s.ron gives the hands' grips" % key)
		var scene_key := "pistol" if key == "m1911" else key
		var view := (
			(load("res://scenes/weapons/%s_view.tscn" % scene_key) as PackedScene).instantiate()
		)
		if found != null and view != null:
			var right: Vector3 = parse.call(found.get_string(1))
			var left: Vector3 = parse.call(found.get_string(2))
			var marker := func(name: String) -> Vector3:
				return (view.get_node(name) as Node3D).position
			_check(
				(
					right.distance_to(marker.call("GripRight")) < 0.001
					and left.distance_to(marker.call("GripLeft")) < 0.001
					and absf(float(found.get_string(3)) - float(view.get("grip_rake"))) < 0.01
				),
				"%s.ron's grips match %s_view.tscn's markers" % [key, scene_key]
			)
		if view != null:
			view.free()


func _measure(world: Node3D, spec: Dictionary) -> void:
	var player := REMOTE_SCENE.instantiate() as RemotePlayer
	world.add_child(player)
	var rig := player.soldier()
	var skeleton := rig.find_child("GeneralSkeleton", true, false) as Skeleton3D
	var body := skeleton.get_node("Body") as MeshInstance3D
	_poses.clear()
	var capture := func() -> void:
		_poses.resize(skeleton.get_bone_count())
		for i in skeleton.get_bone_count():
			_poses[i] = skeleton.get_bone_global_pose(i)
	skeleton.skeleton_updated.connect(capture)
	var mesh := _mesh_data(body, skeleton)

	var state: Dictionary = spec["state"]
	var velocity: Vector3 = spec.get("velocity", Vector3.ZERO)
	var yaw_rate: float = spec.get("yaw_rate", 0.0)
	var settle: int = spec.get("settle", 60)
	var samples: Array = spec.get("samples", [0])
	var feet := Vector3.ZERO
	var yaw: float = state.get("yaw", 0.0)
	var last: int = samples.max()
	var worst := {}
	for zone in ZONES:
		worst[zone] = {"axis": 0.0, "cover": 1.0, "out": 0.0}
	for frame in settle + last + 1:
		feet += velocity * DT
		yaw += yaw_rate * DT
		var s := state.duplicate()
		if frame >= settle:
			# What starts once the body has settled (a reload, a mantle).
			s.merge(spec.get("then", {}), true)
		s["feet"] = feet
		s["yaw"] = yaw
		player.apply_test_state(s, DT)
		await process_frame
		var k := frame - settle
		if k < 0 or not k in samples or _poses.is_empty():
			continue
		var shapes: Array = player.hitboxes().shapes()
		var to_body := _to_body(feet, yaw)
		var result := _score(skeleton, mesh, shapes, to_body)
		for zone in ZONES:
			var w: Dictionary = worst[zone]
			var r: Dictionary = result[zone]
			w["axis"] = maxf(w["axis"], r["axis"])
			w["cover"] = minf(w["cover"], r["cover"])
			w["out"] = maxf(w["out"], r["out"])
	skeleton.skeleton_updated.disconnect(capture)
	player.queue_free()
	await process_frame
	_table.append({"name": spec["name"], "zones": worst, "phase": spec.get("phase", "")})


func _to_body(feet: Vector3, yaw: float) -> Transform3D:
	return Transform3D(Basis(Vector3.UP, yaw), feet).affine_inverse()


## The body mesh's vertices with their skin binds and the zone of their heaviest bone.
func _mesh_data(body: MeshInstance3D, skeleton: Skeleton3D) -> Dictionary:
	var arrays := body.mesh.surface_get_arrays(0)
	var vertices: PackedVector3Array = arrays[Mesh.ARRAY_VERTEX]
	var bones: PackedInt32Array = arrays[Mesh.ARRAY_BONES]
	var weights: PackedFloat32Array = arrays[Mesh.ARRAY_WEIGHTS]
	var per := bones.size() / vertices.size()
	var skin := body.skin
	var bind_bones: Array[int] = []
	var bind_poses: Array[Transform3D] = []
	for i in skin.get_bind_count():
		var bone := skin.get_bind_bone(i)
		if bone < 0:
			bone = skeleton.find_bone(skin.get_bind_name(i))
		bind_bones.append(bone)
		bind_poses.append(skin.get_bind_pose(i))
	var picked := []
	for v in range(0, vertices.size(), VERTEX_STEP):
		var heaviest := 0
		var influences := []
		for j in per:
			var w := weights[v * per + j]
			if w <= 0.0:
				continue
			influences.append([bones[v * per + j], w])
			if w > weights[v * per + heaviest]:
				heaviest = j
		var zone := _zone_of(skeleton.get_bone_name(bind_bones[bones[v * per + heaviest]]))
		if zone.is_empty():
			continue
		picked.append({"vertex": vertices[v], "influences": influences, "zone": zone})
	return {"vertices": picked, "bind_bones": bind_bones, "bind_poses": bind_poses}


func _zone_of(bone: String) -> String:
	if bone == "Head":
		return "head"
	if bone in ["Neck", "UpperChest", "Chest", "LeftShoulder", "RightShoulder"]:
		return "chest"
	if bone in ["Spine", "Hips"]:
		return "stomach"
	if bone.contains("UpperLeg") or bone.contains("LowerLeg"):
		return "leg"
	if bone.contains("Foot") or bone.contains("Toes"):
		return ""  # Below the shins' caps: boots are not a hit zone.
	return "arm"


## Skinned vertex positions in body space, by zone.
func _skin(mesh: Dictionary, to_body: Transform3D, skeleton: Skeleton3D) -> Dictionary:
	var skel_to_body := to_body * skeleton.global_transform
	var bind_bones: Array[int] = mesh["bind_bones"]
	var bind_poses: Array[Transform3D] = mesh["bind_poses"]
	var palette: Array[Transform3D] = []
	for i in bind_bones.size():
		palette.append(skel_to_body * _poses[bind_bones[i]] * bind_poses[i])
	var out := {}
	for zone in ZONES:
		out[zone] = []
	for entry: Dictionary in mesh["vertices"]:
		var p := Vector3.ZERO
		var v: Vector3 = entry["vertex"]
		for inf: Array in entry["influences"]:
			p += (palette[inf[0]] * v) * float(inf[1])
		(out[entry["zone"]] as Array).append(p)
	return out


func _bone(skeleton: Skeleton3D, to_body: Transform3D, name: String) -> Vector3:
	return to_body * skeleton.global_transform * _poses[skeleton.find_bone(name)].origin


## Each hit zone shape's reference on the skeleton: the bone segment it stands for.
func _references(skeleton: Skeleton3D, to_body: Transform3D, head_centre: Vector3) -> Array:
	var b := func(name: String) -> Vector3: return _bone(skeleton, to_body, name)
	var chest: Vector3 = b.call("Chest")
	var upper: Vector3 = b.call("UpperChest")
	var neck: Vector3 = b.call("Neck")
	var spine: Vector3 = b.call("Spine")
	var hips: Vector3 = b.call("Hips")
	return [
		[head_centre, head_centre],
		[neck, b.call("Head")],
		[chest.lerp(upper, 0.3), upper.lerp(neck, 0.5)],
		[hips.lerp(spine, 0.3), spine.lerp(chest, 0.6)],
		[b.call("LeftUpperArm"), b.call("LeftLowerArm")],
		[b.call("LeftLowerArm"), _palm(b, "Left")],
		[b.call("RightUpperArm"), b.call("RightLowerArm")],
		[b.call("RightLowerArm"), _palm(b, "Right")],
		[b.call("LeftUpperLeg"), b.call("LeftLowerLeg")],
		[b.call("LeftLowerLeg"), b.call("LeftFoot")],
		[b.call("RightUpperLeg"), b.call("RightLowerLeg")],
		[b.call("RightLowerLeg"), b.call("RightFoot")],
		[b.call("LeftUpperLeg").lerp(hips, 0.5), b.call("RightUpperLeg").lerp(hips, 0.5)],
	]


## The middle of a hand: between the wrist and the knuckles.
func _palm(b: Callable, side: String) -> Vector3:
	return (b.call(side + "Hand") as Vector3).lerp(b.call(side + "MiddleProximal"), 0.5)


func _score(
	skeleton: Skeleton3D, mesh: Dictionary, shapes: Array, to_body: Transform3D
) -> Dictionary:
	var skinned := _skin(mesh, to_body, skeleton)
	var head_centre := _centroid(skinned["head"])
	var refs := _references(skeleton, to_body, head_centre)
	var result := {}
	for zone in ZONES:
		result[zone] = {"axis": 0.0, "cover": 1.0, "out": 0.0}
	for i in shapes.size():
		var shape: Array = shapes[i]
		var zone: String = shape[0]
		var a: Vector3 = to_body * (shape[1] as Vector3)
		var c: Vector3 = to_body * (shape[2] as Vector3)
		var ref: Array = refs[i]
		var err := maxf(_segment_gap(ref[0], ref[1], a, c), _segment_gap(a, c, ref[0], ref[1]))
		result[zone]["axis"] = maxf(result[zone]["axis"], err)
		if OS.get_environment("HITBOX_FIT_SHAPES") != "":
			print(
				(
					"shape %d %s %.1f cm  ref %s %s  box %s %s"
					% [i, zone, err * 100.0, ref[0], ref[1], a, c]
				)
			)
	for zone in ZONES:
		var points: Array = skinned[zone]
		if points.is_empty():
			continue
		var outs := PackedFloat32Array()
		var inside := 0
		for p: Vector3 in points:
			var d := INF
			for shape: Array in shapes:
				if shape[0] != zone:
					continue
				var a: Vector3 = to_body * (shape[1] as Vector3)
				var c: Vector3 = to_body * (shape[2] as Vector3)
				d = minf(d, _point_segment(p, a, c) - float(shape[3]))
			if d <= 0.01:
				inside += 1
			outs.append(maxf(d, 0.0))
		outs.sort()
		result[zone]["cover"] = float(inside) / points.size()
		result[zone]["out"] = outs[int(outs.size() * 0.9)]
	return result


func _centroid(points: Array) -> Vector3:
	var sum := Vector3.ZERO
	for p: Vector3 in points:
		sum += p
	return sum / maxf(points.size(), 1)


## The largest distance from points along a-b to the segment c-d.
func _segment_gap(a: Vector3, b: Vector3, c: Vector3, d: Vector3) -> float:
	var gap := 0.0
	for k in 9:
		gap = maxf(gap, _point_segment(a.lerp(b, k / 8.0), c, d))
	return gap


func _point_segment(p: Vector3, a: Vector3, b: Vector3) -> float:
	var ab := b - a
	var len_sq := ab.length_squared()
	if len_sq < 1e-10:
		return p.distance_to(a)
	var t := clampf((p - a).dot(ab) / len_sq, 0.0, 1.0)
	return p.distance_to(a + ab * t)


# --- Generating the base poses ------------------------------------------------


## Writes `rust/sim/src/hitbox/poses.rs` to HITBOX_FIT_DUMP: each base pose the
## hit zones start from, averaged over its clip, with level aim.
func _generate(world: Node3D) -> void:
	var bases := [
		["STAND", {}, Vector3.ZERO, 60, 1],
		["WALK", {}, Vector3(0, 0, -1.06), 60, 240],
		["JOG", {}, Vector3(0, 0, -3.0), 60, 240],
		["SPRINT", {"sprinting": true}, Vector3(0, 0, -6.0), 60, 240],
		["CROUCH", {"stance": "crouching"}, Vector3.ZERO, 60, 1],
		["CROUCH_WALK", {"stance": "crouching"}, Vector3(0, 0, -1.2), 60, 240],
		["SLIDE", {"stance": "sliding"}, Vector3(0, 0, -7.0), 40, 1],
		["AIR", {"on_floor": false}, Vector3.ZERO, 30, 20],
		# A mantle up a metre: about 25 frames, from a settled stance.
		["CLIMB", {}, Vector3.ZERO, 60, 24, {"mantling": true}],
	]
	var out := PackedStringArray()
	out.append("//! The base poses the hit zones start from, measured from the soldier's")
	out.append("//! animations with level aim (body space: +X right, +Y up, -Z forward, origin")
	out.append("//! at the feet). Generated by `godot/tests/hitbox_fit_test.gd` (see its")
	out.append("//! header) when the animations change, then `cargo fmt`. Do not edit by hand.")
	out.append("")
	out.append(
		'#![allow(clippy::approx_constant, reason = "measured positions, not math constants")]'
	)
	out.append("")
	out.append("use super::{BasePose, m, v};")
	var extra := {}
	for base: Array in bases:
		var then: Dictionary = base[5] if base.size() > 5 else {}
		var avg := await _average_pose(world, base[1], base[2], base[3], base[4], then, extra)
		out.append("")
		out.append("pub(super) const %s: BasePose = BasePose {" % base[0])
		out.append("    hips: %s," % _v(avg["Hips"]))
		out.append(
			(
				"    spine: [%s],"
				% ", ".join(
					["Spine", "Chest", "UpperChest", "Neck", "Head"].map(
						func(n: String) -> String: return _v(avg[n])
					)
				)
			)
		)
		var head: Basis = avg["head_basis"]
		out.append("    head: m(%s, %s, %s)," % [_v(head.x), _v(head.y), _v(head.z)])
		out.append("    shoulders: [%s, %s]," % [_v(avg["LeftUpperArm"]), _v(avg["RightUpperArm"])])
		out.append("    legs: [")
		for side in ["Left", "Right"]:
			out.append(
				(
					"        [%s, %s, %s],"
					% [
						_v(avg[side + "UpperLeg"]),
						_v(avg[side + "LowerLeg"]),
						_v(avg[side + "Foot"])
					]
				)
			)
		out.append("    ],")
		out.append("};")
	out.append("")
	out.append("/// The head's centre (the middle of its skin) in the head bone's space.")
	out.append("pub(super) const HEAD_CENTRE: crate::Vec3 = %s;" % _v(extra["head_centre"]))
	out.append("/// The middle of each hand (between the wrist and the knuckles) in its")
	out.append("/// hand bone's space: left, then right.")
	out.append(
		(
			"pub(super) const HAND_MIDDLE: [crate::Vec3; 2] = [%s, %s];"
			% [_v(extra["hand_middle_left"]), _v(extra["hand_middle_right"])]
		)
	)
	out.append("/// Shoulder to elbow and elbow to wrist, metres.")
	out.append("pub(super) const UPPER_ARM: f32 = %.4f;" % extra["upper_arm"])
	out.append("pub(super) const FOREARM: f32 = %.4f;" % extra["forearm"])
	var file := FileAccess.open(_dump_path, FileAccess.WRITE)
	file.store_string("\n".join(out) + "\n")
	print("hitbox_fit_test: wrote ", _dump_path)


func _v(p: Vector3) -> String:
	return "v(%.4f, %.4f, %.4f)" % [p.x, p.y, p.z]


func _average_pose(
	world: Node3D,
	state: Dictionary,
	velocity: Vector3,
	settle: int,
	frames: int,
	then: Dictionary,
	extra: Dictionary
) -> Dictionary:
	var player := REMOTE_SCENE.instantiate() as RemotePlayer
	world.add_child(player)
	var skeleton := player.soldier().find_child("GeneralSkeleton", true, false) as Skeleton3D
	var body := skeleton.get_node("Body") as MeshInstance3D
	_poses.clear()
	var capture := func() -> void:
		_poses.resize(skeleton.get_bone_count())
		for i in skeleton.get_bone_count():
			_poses[i] = skeleton.get_bone_global_pose(i)
	skeleton.skeleton_updated.connect(capture)
	var mesh := _mesh_data(body, skeleton)
	var sums := {}
	for name: String in POSE_BONES:
		sums[name] = Vector3.ZERO
	var head_sum := Basis(Vector3.ZERO, Vector3.ZERO, Vector3.ZERO)
	var feet := Vector3.ZERO
	for frame in settle + frames:
		feet += velocity * DT
		var s := state.duplicate()
		if frame >= settle:
			s.merge(then, true)
		s["feet"] = feet
		player.apply_test_state(s, DT)
		await process_frame
		if frame < settle:
			continue
		var to_body := _to_body(feet, 0.0)
		var skel_to_body := to_body * skeleton.global_transform
		for name: String in POSE_BONES:
			sums[name] += skel_to_body * _poses[skeleton.find_bone(name)].origin
		var head := skel_to_body * _poses[skeleton.find_bone("Head")]
		head_sum.x += head.basis.x
		head_sum.y += head.basis.y
		head_sum.z += head.basis.z
		if extra.is_empty():
			var skinned := _skin(mesh, to_body, skeleton)
			extra["head_centre"] = head.affine_inverse() * _centroid(skinned["head"])
			var b := func(n: String) -> Vector3:
				return skel_to_body * _poses[skeleton.find_bone(n)].origin
			extra["upper_arm"] = (b.call("LeftUpperArm") as Vector3).distance_to(
				b.call("LeftLowerArm")
			)
			extra["forearm"] = (b.call("LeftLowerArm") as Vector3).distance_to(b.call("LeftHand"))
			for side in ["Left", "Right"]:
				var hand := skel_to_body * _poses[skeleton.find_bone(side + "Hand")]
				var knuckles: Vector3 = b.call(side + "MiddleProximal")
				extra["hand_middle_" + side.to_lower()] = hand.affine_inverse() * knuckles * 0.5
	skeleton.skeleton_updated.disconnect(capture)
	player.queue_free()
	await process_frame
	var avg := {}
	for name: String in POSE_BONES:
		avg[name] = sums[name] / frames
	avg["head_basis"] = head_sum.orthonormalized()
	return avg


func _report() -> void:
	var header := "%-18s" % "state"
	for zone in ZONES:
		header += " | %-7s axis cover  out" % zone
	print(header)
	var totals := {}
	for zone in ZONES:
		totals[zone] = {"axis": 0.0, "cover": 1.0, "out": 0.0, "worst": ""}
	for row: Dictionary in _table:
		var line := "%-18s" % row["name"]
		for zone in ZONES:
			var z: Dictionary = row["zones"][zone]
			line += (
				" | %12.1f %4d%% %4.1f"
				% [z["axis"] * 100.0, int(z["cover"] * 100.0), z["out"] * 100.0]
			)
			var t: Dictionary = totals[zone]
			if z["axis"] > t["axis"]:
				t["axis"] = z["axis"]
				t["worst"] = row["name"]
			t["cover"] = minf(t["cover"], z["cover"])
			t["out"] = maxf(t["out"], z["out"])
			var phase: Dictionary = PHASE.get(row["phase"], {})
			if phase.has(zone):
				# Phase-limited: a looser axis bound, no skin check.
				_check(
					z["axis"] <= phase[zone],
					(
						"%s: %s bones within %.0f cm of the hit zones (%.1f cm)"
						% [row["name"], zone, phase[zone] * 100.0, z["axis"] * 100.0]
					)
				)
				continue
			_check(
				z["axis"] <= AXIS_TOLERANCE,
				(
					"%s: %s bones within %.0f cm of the hit zones (%.1f cm)"
					% [row["name"], zone, AXIS_TOLERANCE * 100.0, z["axis"] * 100.0]
				)
			)
			_check(
				z["out"] <= OUT_TOLERANCE,
				(
					"%s: nine in ten of the %s's vertices within %.1f cm of its zone (%.1f cm)"
					% [row["name"], zone, OUT_TOLERANCE * 100.0, z["out"] * 100.0]
				)
			)
		line += "  " + String(row["phase"])
		print(line)
	for zone in ZONES:
		var t: Dictionary = totals[zone]
		print(
			(
				"%-8s worst axis %.1f cm (%s), lowest cover %d%%, worst out %.1f cm"
				% [zone, t["axis"] * 100.0, t["worst"], int(t["cover"] * 100.0), t["out"] * 100.0]
			)
		)


func _check(ok: bool, what: String) -> void:
	if not ok:
		_failures.append(what)
		print("FAIL: ", what)


func _finish() -> void:
	if _failures.is_empty():
		print("hitbox_fit_test: OK (%d states)" % _table.size())
		quit(0)
	else:
		print("hitbox_fit_test: %d failures" % _failures.size())
		quit(1)
