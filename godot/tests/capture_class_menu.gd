extends SceneTree
## Captures the create-a-class menu for the docs: the class list and the
## editor with its perk tiers, as a high-level player sees them, over the
## match menu's backdrop. Uses its own class and progress files.
## Needs a GPU and a display (or Xvfb):
##   CAPTURE_DIR=/path/to/out godot --path godot --resolution 1600x900 \
##     --audio-driver Dummy --script res://tests/capture_class_menu.gd

const ClassMenu := preload("res://scripts/net/class_menu.gd")
const MenuStyle := preload("res://scripts/ui/menu_style.gd")

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
	# Camos earned on the M4A1: its headshot ladder and longshots.
	ProgressStore.add(320000, {"m4": 60, "pdw9": 100}, false, {"w.m4.hs": 60, "w.m4.long": 12})
	ClassStore.reload()
	var layer := CanvasLayer.new()
	root.add_child(layer)
	var backdrop := ColorRect.new()
	backdrop.color = Color(0.07, 0.08, 0.09)
	backdrop.set_anchors_preset(Control.PRESET_FULL_RECT)
	layer.add_child(backdrop)
	var center := CenterContainer.new()
	center.set_anchors_preset(Control.PRESET_FULL_RECT)
	layer.add_child(center)
	var panel := PanelContainer.new()
	panel.custom_minimum_size = Vector2(760, 0)
	panel.add_theme_stylebox_override("panel", MenuStyle.panel())
	center.add_child(panel)
	var menu: VBoxContainer = ClassMenu.new()
	panel.add_child(menu)
	await _frames(10)
	await _shot("class_list")
	menu.call("_open_editor", "custom:0")
	await _frames(10)
	await _shot("class_editor")
	# Its camo picked: the class card names it.
	var draft: Dictionary = menu.get("_draft")
	draft["primary"] = "m4"
	draft["primary_camo"] = "riptide"
	menu.call("_rebuild_editor")
	await _frames(5)
	var scroll := menu.find_children("*", "ScrollContainer", true, false)
	for found: ScrollContainer in scroll:
		found.scroll_vertical = 220
	await _frames(5)
	await _shot("class_editor_camo")
	for file in [ClassStore.path(), ProgressStore.path()]:
		DirAccess.remove_absolute(ProjectSettings.globalize_path(file))
	quit(0)


func _frames(count: int) -> void:
	for i in count:
		await process_frame


func _shot(name: String) -> void:
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	image.save_png(_out.path_join(name + ".png"))
	print("saved ", name)
