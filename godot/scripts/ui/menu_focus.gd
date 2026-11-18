extends Node
## Autoload `MenuFocus`: keeps every menu usable with a controller. Godot moves
## focus with the D-pad and left stick, and project.godot maps A and B to
## `ui_accept` and `ui_cancel`. Two gaps are left, closed here for every screen:
##
## - Nothing has focus, because a click cleared it or the focused control was
##   freed or hidden. Godot then ignores the D-pad and A. Here the first
##   direction or A press focuses a control instead, and does nothing else:
##   the one that last had focus, or else the one nearest it on the topmost
##   layer. A freed control (a row rebuilt under it) hands focus on at once.
## - Godot moves once per D-pad press or stick tilt, however long it is held.
##   Here a held direction repeats, like a held arrow key.
##
## Both apply only while the cursor is free (a menu is open). In play the mouse
## is captured, and A and B jump and crouch.

const DIRECTIONS: Array[StringName] = [&"ui_up", &"ui_down", &"ui_left", &"ui_right"]
const ACCEPT := &"ui_accept"
## Seconds a direction is held before it repeats, then between repeats.
const REPEAT_DELAY := 0.38
const REPEAT_INTERVAL := 0.085

## The control that last had focus, and where it was on screen.
var _last_focus: WeakRef = weakref(null)
var _last_center := Vector2(-1.0, -1.0)
## The direction held on a controller, and seconds until it next repeats.
var _held := &""
var _repeat_in := 0.0


func _ready() -> void:
	# The pause menu pauses the tree; menus need this most then.
	process_mode = Node.PROCESS_MODE_ALWAYS
	get_viewport().gui_focus_changed.connect(_on_focus_changed)


func _input(event: InputEvent) -> void:
	if not _menu_open():
		_held = &""
		return
	if event is InputEventJoypadButton or event is InputEventJoypadMotion:
		_track_held(event)
	if event.is_echo() or get_viewport().gui_get_focus_owner() != null:
		return
	for action: StringName in DIRECTIONS + [ACCEPT]:
		if event.is_action_pressed(action) and refocus():
			get_viewport().set_input_as_handled()
			return


func _process(delta: float) -> void:
	if _held.is_empty():
		return
	if not _menu_open() or not Input.is_action_pressed(_held):
		_held = &""
		return
	_repeat_in -= delta
	if _repeat_in > 0.0 or get_viewport().gui_get_focus_owner() == null:
		return
	_repeat_in += REPEAT_INTERVAL
	# Through the viewport only: Input's own action state stays as the pad left it.
	var step := InputEventAction.new()
	step.action = _held
	step.pressed = true
	get_viewport().push_input(step)


## Focuses a control on the topmost layer that has one: the control that last
## had focus if it is there, or else the one nearest where focus last was (the
## first one, before anything had focus). Returns false when nothing can take
## focus.
func refocus() -> bool:
	var candidates := _focusable(get_tree().root, 0)
	if candidates.is_empty():
		return false
	var top := -INF
	for entry: Array in candidates:
		top = maxf(top, entry[1])
	var last := _last_focus.get_ref() as Control
	var best: Control = null
	var best_distance := INF
	for entry: Array in candidates:
		if entry[1] < top:
			continue
		var control: Control = entry[0]
		if control == last:
			best = control
			break
		var distance := 0.0
		if _last_center.x >= 0.0:
			distance = control.get_global_rect().get_center().distance_to(_last_center)
		if distance < best_distance:
			best = control
			best_distance = distance
	best.grab_focus()
	return true


func _menu_open() -> bool:
	return Input.mouse_mode == Input.MOUSE_MODE_VISIBLE


func _track_held(event: InputEvent) -> void:
	for action: StringName in DIRECTIONS:
		if event.is_action_pressed(action):
			if _held != action:
				_held = action
				_repeat_in = REPEAT_DELAY
		elif event.is_action_released(action) and _held == action:
			_held = &""


func _on_focus_changed(control: Control) -> void:
	_last_focus = weakref(control)
	_last_center = control.get_global_rect().get_center()
	if not control.tree_exiting.is_connected(_on_focused_exiting):
		control.tree_exiting.connect(_on_focused_exiting.bind(control), CONNECT_ONE_SHOT)


## A focused control leaving the tree, often freed with its row rebuilt in the
## same frame: once the new controls are laid out, the nearest takes focus.
## Only while its parent stays, so a scene change hands focus to nothing.
func _on_focused_exiting(control: Control) -> void:
	if get_viewport().gui_get_focus_owner() != control or not _menu_open():
		return
	var parent: WeakRef = weakref(control.get_parent())
	await get_tree().process_frame
	var row := parent.get_ref() as Node
	if row == null or not row.is_inside_tree() or row.is_queued_for_deletion():
		return
	if get_viewport().gui_get_focus_owner() == null and _menu_open():
		refocus()


## Every visible control under `node` that takes focus, with its canvas layer.
## Skips hidden branches and popup windows, which handle their own focus.
func _focusable(node: Node, layer: int) -> Array[Array]:
	var found: Array[Array] = []
	if node is CanvasLayer:
		if not (node as CanvasLayer).visible:
			return found
		layer = (node as CanvasLayer).layer
	elif node is CanvasItem and not (node as CanvasItem).visible:
		return found
	elif node is Window and node != get_tree().root:
		return found
	if node is Control and _can_focus(node as Control):
		found.append([node, layer])
	for child in node.get_children():
		found.append_array(_focusable(child, layer))
	return found


func _can_focus(control: Control) -> bool:
	if control.focus_mode != Control.FOCUS_ALL or not control.is_visible_in_tree():
		return false
	return not (control is BaseButton and (control as BaseButton).disabled)
