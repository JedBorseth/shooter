extends Node
## Keeps a menu footer's key caps in step with the device used last
## (`Settings.menu_controller`): the keyboard key in its cap ("ENTER"), or the
## controller button's glyph, as Call of Duty's menus swap their button hints
## the moment a controller is touched. Add it to the menu and `bind` each cap
## Label to the menu action it shows.

const Glyphs := preload("res://scripts/ui/glyphs.gd")

## Height of a glyph in the footer; the button fills three quarters of it,
## about a key cap's height.
const GLYPH_SIZE := 30.0

## [cap Label, glyph TextureRect, action] for each bound cap.
var _caps: Array = []


func _ready() -> void:
	Settings.prompts_changed.connect(refresh)
	refresh()


## Shows `action`'s controller glyph beside `cap`, in its place, while the
## controller is the device used last.
func bind(cap: Label, action: StringName) -> void:
	var icon := TextureRect.new()
	icon.name = cap.name + "Glyph"
	icon.expand_mode = TextureRect.EXPAND_IGNORE_SIZE
	icon.stretch_mode = TextureRect.STRETCH_KEEP_ASPECT_CENTERED
	icon.custom_minimum_size = Vector2(GLYPH_SIZE, GLYPH_SIZE)
	icon.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	icon.mouse_filter = Control.MOUSE_FILTER_IGNORE
	cap.add_sibling(icon)
	_caps.append([cap, icon, action])
	refresh()


func refresh() -> void:
	for entry: Array in _caps:
		var key: String = Settings.menu_key(entry[2])
		var glyph: Texture2D = Glyphs.texture(key) if Glyphs.is_pad(key) else null
		(entry[1] as TextureRect).texture = glyph
		(entry[1] as TextureRect).visible = glyph != null
		(entry[0] as Label).visible = glyph == null
