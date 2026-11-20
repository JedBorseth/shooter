extends CanvasLayer
## Subtitles for calls and sounds that carry information (made by the
## `Settings` autoload, shown only while Settings, Audio, "Subtitles" is on).
## Call of Duty subtitles its announcer and dialogue; this game has no voice
## lines yet, so for now the captions name the match's audio stings that no
## banner already announces ("[Sting] 30 seconds left"). A voice line added
## later captions itself with `Settings.caption(speaker, text)`.
##
## Lines stack bottom centre, above the controls legend, newest at the
## bottom, and each fades after a few seconds.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")

## Seconds a line stays before it fades.
const SHOW_TIME := 4.0
const MAX_LINES := 3

## The `Settings` autoload, which makes this node.
var settings: Node
var _box: VBoxContainer


func _ready() -> void:
	layer = 19
	process_mode = Node.PROCESS_MODE_ALWAYS
	_box = VBoxContainer.new()
	_box.name = "Lines"
	_box.mouse_filter = Control.MOUSE_FILTER_IGNORE
	_box.alignment = BoxContainer.ALIGNMENT_END
	_box.set_anchors_and_offsets_preset(Control.PRESET_CENTER_BOTTOM)
	_box.grow_horizontal = Control.GROW_DIRECTION_BOTH
	_box.grow_vertical = Control.GROW_DIRECTION_BEGIN
	# Above the controls legend and the interact prompt band.
	_box.position.y -= 120.0
	add_child(_box)


## Shows `text`, said by `speaker` ("" for none), if subtitles are on.
func say(speaker: String, text: String) -> void:
	if settings == null or not settings.get(&"subtitles") or text == "":
		return
	var panel := PanelContainer.new()
	panel.mouse_filter = Control.MOUSE_FILTER_IGNORE
	panel.size_flags_horizontal = Control.SIZE_SHRINK_CENTER
	panel.add_theme_stylebox_override(&"panel", UiTheme.plate(UiTheme.PLATE_STRONG, 14.0, 6.0))
	var label := RichTextLabel.new()
	label.bbcode_enabled = true
	label.fit_content = true
	label.autowrap_mode = TextServer.AUTOWRAP_OFF
	label.mouse_filter = Control.MOUSE_FILTER_IGNORE
	label.add_theme_font_size_override(&"normal_font_size", UiTheme.SIZE_BODY)
	label.add_theme_font_size_override(&"bold_font_size", UiTheme.SIZE_BODY)
	label.add_theme_font_override(&"bold_font", UiTheme.font(UiTheme.LABEL))
	var escaped := text.replace("[", "[lb]")
	label.text = (
		"[b][color=#%s]%s[/color][/b]  %s" % [UiTheme.ACCENT.to_html(false), speaker, escaped]
		if speaker != ""
		else escaped
	)
	panel.add_child(label)
	panel.set_meta(&"text", text)
	_box.add_child(panel)
	while _box.get_child_count() > MAX_LINES:
		var oldest := _box.get_child(0)
		_box.remove_child(oldest)
		oldest.queue_free()
	var tween := panel.create_tween()
	tween.tween_interval(SHOW_TIME)
	tween.tween_property(panel, ^"modulate:a", 0.0, 0.4)
	tween.tween_callback(panel.queue_free)


## The lines showing now (tests).
func lines() -> PackedStringArray:
	var out := PackedStringArray()
	for panel: Node in _box.get_children():
		if not panel.is_queued_for_deletion():
			out.append(String(panel.get_meta(&"text", "")))
	return out
