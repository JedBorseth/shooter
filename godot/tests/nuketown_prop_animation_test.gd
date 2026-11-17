extends SceneTree
## Exercise the generated Nuketown prop animation data and helper against the
## real flattened glTF models. Runs headless:
##   godot --headless --path godot --script res://tests/nuketown_prop_animation_test.gd

const BEHAVIOR := "res://assets/maps/nuketown/props/behavior.json"
const MODELS := "res://assets/maps/nuketown/props/models/"
const ANIMATION := preload("res://scripts/nuketown_prop_animation.gd")

var _failures := 0
var _behavior: Dictionary = {}


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var text := FileAccess.get_file_as_string(BEHAVIOR)
	if text.is_empty():
		_check(false, "behavior.json exists")
		_finish()
		return
	var parsed = JSON.parse_string(text)
	_check(parsed is Dictionary, "behavior.json parses")
	if not parsed is Dictionary:
		_finish()
		return
	_behavior = parsed
	_check(_behavior.get("schema", "") == "nuketown-prop-behavior/v1", "behavior schema")
	_check(
		(_behavior.get("destructibles", []) as Array).size() == 9, "nine destructible definitions"
	)
	_check((_behavior.get("animations", []) as Array).size() == 8, "eight animation clips")
	_check((_behavior.get("models", {}) as Dictionary).size() >= 8, "candidate model bind tables")
	_check(
		_behavior.get("destructibles", [])[0].has("model_info"),
		"destructible definitions keep their model bone metadata",
	)

	_test_model_tables()
	_test_gate_interpolation()
	_test_loop_and_one_shot()
	_test_display_glass()
	_test_car_root_motion()
	_test_car02_root_motion()
	_test_sprinkler_root_motion()
	_finish()


func _test_model_tables() -> void:
	var models: Dictionary = _behavior.get("models", {})
	for model_name in models:
		var model: Node3D = _load_model(String(model_name))
		if model == null:
			continue
		var helper = ANIMATION.new()
		helper.configure(model, String(model_name), _behavior)
		var table: Dictionary = models[model_name]
		var bones: Array = table.get("bones", [])
		_check(not bones.is_empty(), model_name + " has bone binds")
		_check(
			String(table.get("root", "")) == "tag_origin",
			model_name + " roots at tag_origin",
		)
		_check(helper.configured_model() == String(model_name), model_name + " configures")
		_check(helper.clip_names().size() > 0, model_name + " has a playable clip")
		_check(
			(
				not helper.has_clip("fxanim_mp_nuked2025_dome_anim")
				or String(model_name) == "fxanim_mp_nuked2025_dome_mod"
			),
			model_name + " filters foreign clips"
		)
		model.free()


func _test_gate_interpolation() -> void:
	var model: Node3D = _load_model("fxanim_mp_nuked2025_gate_sign02_mod")
	if model == null:
		return
	var helper = ANIMATION.new()
	helper.configure(model, "fxanim_mp_nuked2025_gate_sign02_mod", _behavior)
	var sign_node := model.find_child("sign02_jnt", true, false) as Node3D
	_check(sign_node != null, "gate model exposes sign02_jnt")
	_check(helper.has_clip("fxanim_mp_nuked2025_gate_sign_anim"), "gate model has gate_sign clip")
	_check(not helper.has_clip("fxanim_mp_nuked2025_dome_anim"), "gate model rejects dome clip")
	if sign_node != null:
		helper.play("fxanim_mp_nuked2025_gate_sign_anim")
		helper.sample(12.0 / 30.0)
		var first := sign_node.quaternion
		helper.sample(16.0 / 30.0)
		var last := sign_node.quaternion
		helper.sample(14.0 / 30.0)
		var middle := sign_node.quaternion
		var expected := first.slerp(last, 0.5)
		_check(
			_quat_close(middle, expected, 0.01),
			"gate sign pose slerps between source keys (%.3f rad)" % middle.angle_to(expected),
		)
		_check(
			middle.angle_to(first) > 0.001 and middle.angle_to(last) > 0.001,
			"gate sign midpoint is between its neighbouring poses",
		)
	model.free()


func _test_loop_and_one_shot() -> void:
	var gate: Node3D = _load_model("fxanim_mp_nuked2025_gate_sign02_mod")
	if gate != null:
		var helper = ANIMATION.new()
		helper.configure(gate, "fxanim_mp_nuked2025_gate_sign02_mod", _behavior)
		var node := gate.find_child("sign01_jnt", true, false) as Node3D
		if node != null:
			helper.play("fxanim_mp_nuked2025_gate_sign_anim")
			helper.sample(0.1)
			var expected := node.transform
			helper.play("fxanim_mp_nuked2025_gate_sign_anim")
			helper.advance(helper.clip_duration() + 0.1)
			_check(helper.is_looping(), "gate sign loops by default")
			_check(
				_transform_close(node.transform, expected, 0.001),
				"looping gate sign wraps after one duration",
			)
		gate.free()

	var dome: Node3D = _load_model("fxanim_mp_nuked2025_dome_mod")
	if dome != null:
		var helper = ANIMATION.new()
		helper.configure(dome, "fxanim_mp_nuked2025_dome_mod", _behavior)
		var panel := dome.find_child("panel_01_jnt", true, false) as Node3D
		_check(panel != null, "dome model exposes panel_01_jnt")
		if panel != null:
			helper.play("fxanim_mp_nuked2025_dome_anim")
			_check(not helper.is_looping(), "dome does not loop by default")
			helper.sample(0.0)
			var at_zero := panel.transform
			helper.sample(helper.clip_duration())
			var at_end := panel.transform
			_check(
				not _transform_close(at_zero, at_end, 0.01),
				"dome moves between frame zero and its end",
			)
			helper.play("fxanim_mp_nuked2025_dome_anim")
			helper.advance(helper.clip_duration() + 0.5)
			_check(not helper.is_playing(), "dome stops at the end")
			_check(
				_transform_close(panel.transform, at_end, 0.001),
				"a finished one-shot dome holds its final pose",
			)
		dome.free()


func _test_display_glass() -> void:
	var model: Node3D = _load_model("fxanim_mp_nuked2025_display_glass_mod")
	if model == null:
		return
	var helper = ANIMATION.new()
	helper.configure(model, "fxanim_mp_nuked2025_display_glass_mod", _behavior)
	var pristine := model.find_child("car01_glass_prestine_jnt", true, false) as Node3D
	var chunk := model.find_child("glass_chunk01_jnt", true, false) as Node3D
	_check(pristine != null and chunk != null, "display glass exposes pristine and chunk nodes")
	var clip := _clip("fxanim_mp_nuked2025_display_glass_anim")
	_check(not clip.is_empty(), "display glass clip is present")
	if pristine != null and chunk != null and not clip.is_empty():
		helper.play("fxanim_mp_nuked2025_display_glass_anim")
		_check(pristine.visible, "display glass frame zero shows the intact pane")
		_check(not chunk.visible, "display glass frame zero hides debris chunks")
		_check(
			pristine.transform.origin.length() < 0.001,
			"display glass sample zero leaves the pristine pane at its bind position",
		)
		var shatter := float(clip.get("visibility", {}).get("shatter_seconds", 0.3))
		helper.sample(shatter + 0.01)
		_check(not pristine.visible, "display glass hides the pane at shatter")
		_check(chunk.visible, "display glass reveals debris at shatter")
		helper.stop()
		_check(
			pristine.visible and chunk.visible,
			"stopping restores the display glass meshes it hid",
		)
	model.free()


func _test_car_root_motion() -> void:
	var model: Node3D = _load_model("veh_t6_nuketown_2020_car01_whole")
	if model == null:
		return
	var helper = ANIMATION.new()
	helper.configure(model, "veh_t6_nuketown_2020_car01_whole", _behavior)
	var body := model.find_child("tag_body", true, false) as Node3D
	var clip := _clip("fxanim_mp_nuked2025_car01_anim")
	_check(body != null, "car model exposes tag_body")
	if body != null and not clip.is_empty():
		body.visible = false
		helper.play("fxanim_mp_nuked2025_car01_anim")
		_check(not body.visible, "car animation leaves hidden damage pieces to the props manager")
		_check(not helper.is_looping(), "car flip is a one-shot")
		helper.sample(helper.clip_duration())
		var root_motion: Dictionary = clip.get("root_motion", {})
		var translation_track: Dictionary = root_motion.get("translation", {})
		var rotation_track: Dictionary = root_motion.get("rotation", {})
		if not translation_track.is_empty():
			var expected := _track_vec(translation_track, helper.clip_duration(), float(clip.fps))
			_check(
				body.transform.origin.distance_to(expected) < 0.001,
				"car root translation is converted to y-up metres (%s)" % expected,
			)
			_check(expected.z < -8.0 and expected.y > 1.0, "car launches forward and up")
		if not rotation_track.is_empty():
			var expected_rotation := _track_quat(
				rotation_track, helper.clip_duration(), float(clip.fps)
			)
			_check(
				_quat_close(body.quaternion, expected_rotation, 0.01),
				"car root rotation becomes a y-up flip",
			)
	model.free()


func _test_car02_root_motion() -> void:
	var model: Node3D = _load_model("veh_t6_nuketown_2020_car02_whole")
	if model == null:
		return
	var helper = ANIMATION.new()
	helper.configure(model, "veh_t6_nuketown_2020_car02_whole", _behavior)
	var body := model.find_child("tag_body", true, false) as Node3D
	var clip := _clip("fxanim_mp_nuked2025_car02_anim")
	_check(body != null, "car02 model exposes tag_body")
	if body != null and not clip.is_empty():
		helper.play("fxanim_mp_nuked2025_car02_anim")
		helper.sample(helper.clip_duration())
		var root_motion: Dictionary = clip.get("root_motion", {})
		var translation_track: Dictionary = root_motion.get("translation", {})
		var rotation_track: Dictionary = root_motion.get("rotation", {})
		if not translation_track.is_empty():
			var expected := _track_vec(translation_track, helper.clip_duration(), float(clip.fps))
			_check(
				body.transform.origin.distance_to(expected) < 0.001,
				"car02 root translation is converted to y-up metres (%s)" % expected,
			)
		if not rotation_track.is_empty():
			var expected_rotation := _track_quat(
				rotation_track, helper.clip_duration(), float(clip.fps)
			)
			_check(
				_quat_close(body.quaternion, expected_rotation, 0.01),
				"car02 compound root rotation is conjugated into y-up",
			)
	model.free()


func _test_sprinkler_root_motion() -> void:
	var model: Node3D = _load_model("fxanim_mp_nuked2025_sprinkler_mod")
	if model == null:
		return
	var helper = ANIMATION.new()
	helper.configure(model, "fxanim_mp_nuked2025_sprinkler_mod", _behavior)
	var arm := model.find_child("arm_jnt", true, false) as Node3D
	var clip := _clip("fxanim_mp_nuked2025_sprinkler_anim")
	_check(arm != null, "sprinkler model exposes arm_jnt")
	if arm != null and not clip.is_empty():
		helper.play("fxanim_mp_nuked2025_sprinkler_anim")
		_check(helper.is_looping(), "sprinkler loops")
		helper.sample(1.0)
		var root_motion: Dictionary = clip.get("root_motion", {})
		var translation_track: Dictionary = root_motion.get("translation", {})
		var rotation_track: Dictionary = root_motion.get("rotation", {})
		if not translation_track.is_empty():
			var expected := _track_vec(translation_track, 1.0, float(clip.fps))
			_check(
				arm.transform.origin.distance_to(expected) < 0.001,
				"sprinkler root translation follows its track (%s)" % expected,
			)
		if not rotation_track.is_empty():
			var expected_rotation := _track_quat(rotation_track, 1.0, float(clip.fps))
			_check(
				_quat_close(arm.quaternion, expected_rotation, 0.01),
				"sprinkler root spin follows its track",
			)
	model.free()


func _load_model(model_name: String) -> Node3D:
	var document := GLTFDocument.new()
	var state := GLTFState.new()
	var error := document.append_from_file(MODELS + model_name + ".gltf", state)
	if error != OK:
		_check(false, "load generated model " + model_name)
		return null
	var scene := document.generate_scene(state)
	if not scene is Node3D:
		_check(false, "generated model has a Node3D root: " + model_name)
		if scene != null:
			scene.free()
		return null
	return scene as Node3D


func _clip(name: String) -> Dictionary:
	for item in _behavior.get("animations", []):
		if item is Dictionary and String(item.get("name", "")) == name:
			return item
	return {}


func _track_vec(track: Dictionary, time_seconds: float, fps: float) -> Vector3:
	var indices: Array = track.get("indices", [])
	var values: Array = track.get("values", [])
	if values.is_empty():
		return Vector3.ZERO
	var frame := time_seconds * fps
	var last := mini(indices.size(), values.size()) - 1
	if indices.size() < 2 or frame <= float(indices[0]):
		return _vec(values[0])
	if frame >= float(indices[last]):
		return _vec(values[last])
	for index in last:
		if frame <= float(indices[index + 1]):
			var alpha := (
				(frame - float(indices[index]))
				/ maxf(float(indices[index + 1]) - float(indices[index]), 1e-6)
			)
			return _vec(values[index]).lerp(_vec(values[index + 1]), alpha)
	return _vec(values[last])


func _track_quat(track: Dictionary, time_seconds: float, fps: float) -> Quaternion:
	var indices: Array = track.get("indices", [])
	var values: Array = track.get("values", [])
	if values.is_empty():
		return Quaternion.IDENTITY
	var frame := time_seconds * fps
	var last := mini(indices.size(), values.size()) - 1
	if indices.size() < 2 or frame <= float(indices[0]):
		return _quat(values[0])
	if frame >= float(indices[last]):
		return _quat(values[last])
	for index in last:
		if frame <= float(indices[index + 1]):
			var alpha := (
				(frame - float(indices[index]))
				/ maxf(float(indices[index + 1]) - float(indices[index]), 1e-6)
			)
			return _quat(values[index]).slerp(_quat(values[index + 1]), alpha).normalized()
	return _quat(values[last])


func _vec(value: Array) -> Vector3:
	return Vector3(float(value[0]), float(value[1]), float(value[2]))


func _quat(value: Array) -> Quaternion:
	return (
		Quaternion(float(value[0]), float(value[1]), float(value[2]), float(value[3])).normalized()
	)


func _quat_close(a: Quaternion, b: Quaternion, tolerance: float) -> bool:
	return a.angle_to(b) <= tolerance


func _transform_close(a: Transform3D, b: Transform3D, tolerance: float) -> bool:
	if a.origin.distance_to(b.origin) > tolerance:
		return false
	return _quat_close(
		a.basis.get_rotation_quaternion(), b.basis.get_rotation_quaternion(), tolerance
	)


func _check(ok: bool, description: String) -> void:
	print("%s %s" % ["PASS" if ok else "FAIL", description])
	if not ok:
		_failures += 1


func _finish() -> void:
	print("Nuketown prop animation test: %d failure(s)" % _failures)
	quit(1 if _failures else 0)
