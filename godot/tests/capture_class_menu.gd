extends SceneTree
## Captures Create-a-Class for the docs and design review, full screen in the
## match menu (`match_menu.gd`): the class list, a default class, the editor
## on each kind of slot (weapons with their stat bars, attachments, grenades,
## perks, wildcards), the scorestreaks, and a new player's locked classes.
## Uses its own class and progress files. BACKDROP is an optional 1600x900
## still to draw behind it (a `clean` capture from `capture_ui.gd`).
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_class_menu.gd

var _out := OS.get_environment("CAPTURE_DIR")


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	if _out.is_empty():
		push_error("Set CAPTURE_DIR to an output directory.")
		quit(1)
		return
	DirAccess.make_dir_recursive_absolute(_out)
	ClassStore.use_file("user://capture_class_menu.cfg")
	ProgressStore.use_file("user://capture_class_menu_progress.cfg")
	for file in [ClassStore.path(), ProgressStore.path()]:
		DirAccess.remove_absolute(ProjectSettings.globalize_path(file))
	ClassStore.reload()
	ProgressStore.reload()
	# The pointer out of the way: pointing at a tile focuses it.
	Input.warp_mouse(Vector2.ZERO)
	# A new player first: custom classes locked.
	var menu := await _open_menu()
	await _shot("class_00_new_player")
	menu.get_parent().queue_free()
	await _frames(2)
	# Far along, with a few items still locked.
	ProgressStore.add(48000, {"m4": 60, "pdw9": 100})
	ClassStore.reload()
	menu = await _open_menu()
	var classes: VBoxContainer = menu.get("class_menu")
	await _shot("class_01_list")
	(classes.get("_class_buttons")["default:2"] as Button).grab_focus()
	await _frames(6)
	await _shot("class_02_default_focused")
	classes.call("_open_streaks", 0)
	await _frames(6)
	await _shot("class_03_streaks")
	classes.call("close_editor")
	classes.call("_open_editor", "custom:0")
	await _frames(6)
	await _shot("class_04_edit_primary")
	for slot: String in [
		"primary_attachments", "lethal", "tactical", "perk1", "perk2", "wildcards"
	]:
		_focus_slot(classes, slot)
		await _frames(4)
		await _shot("class_05_edit_" + slot)
	# A locked weapon in focus: its lock and level in the detail.
	_focus_slot(classes, "primary")
	await _frames(2)
	var picker: Control = classes.get("_picker")
	for i in 16:
		var card: Button = picker.call("card", i)
		if card != null and String(card.get("lock")) != "":
			card.grab_focus()
			break
	await _frames(6)
	await _shot("class_06_edit_locked")
	for file in [ClassStore.path(), ProgressStore.path()]:
		DirAccess.remove_absolute(ProjectSettings.globalize_path(file))
	quit(0)


func _open_menu() -> Control:
	var layer := CanvasLayer.new()
	root.add_child(layer)
	var still := OS.get_environment("BACKDROP")
	if still != "" and FileAccess.file_exists(still):
		var rect := TextureRect.new()
		rect.set_anchors_preset(Control.PRESET_FULL_RECT)
		rect.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_COVERED
		rect.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
		rect.texture = ImageTexture.create_from_image(Image.load_from_file(still))
		layer.add_child(rect)
	# Loaded here: it reaches autoloads, which a main script cannot preload.
	var menu: Control = load("res://scripts/net/match_menu.gd").new()
	menu.set("mode_name", "Team Deathmatch")
	layer.add_child(menu)
	await _frames(8)
	(menu.get("class_menu") as VBoxContainer).call("focus_first")
	await _frames(6)
	return menu


## Focuses `slot`'s tile in the editor's grid, which opens its items.
func _focus_slot(classes: Control, slot: String) -> void:
	((classes.get("_grid") as Control).call("tile", slot) as Button).grab_focus()


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _shot(name: String) -> void:
	# Past every fade (focus dimming takes about 0.2 s).
	await create_timer(0.35).timeout
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	image.save_png(_out.path_join(name + ".png"))
	print("saved ", name)
