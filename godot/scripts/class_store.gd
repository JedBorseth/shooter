class_name ClassStore
extends RefCounted
## The player's classes, saved in `user://classes.cfg`: five custom classes
## (Black Ops 2's five slots) and which class to spawn with. A custom slot
## starts as a copy of the default class in the same place, as in Call of
## Duty. Classes are dictionaries in `NetClient`'s class shape (see
## `rust/game/src/net/class_dict.rs`) with a `name`.
##
## The server checks every class it is sent; this only keeps what the
## player made between sessions.

const PATH := "user://classes.cfg"
const HEADLESS_PATH := "user://headless_classes.cfg"
const CUSTOM_SLOTS := 5
## Longest class name.
const NAME_CHARS := 20

## Tests and captures point the store at a scratch file with
## `use_file`, so they never touch the player's. Kept as engine metadata,
## which a reloaded script does not reset.
const PATH_META := &"class_store_path"

static var _custom: Array[Dictionary] = []
static var _selected := "default:0"
static var _loaded := false


## The custom classes, loading them on first use.
static func custom() -> Array[Dictionary]:
	_load()
	return _custom


## Every class to choose from: the custom ones, then the defaults. Each is a
## copy with `id` ("custom:2", "default:0") and `name`.
static func all() -> Array[Dictionary]:
	_load()
	var out: Array[Dictionary] = []
	for i in _custom.size():
		var entry := _custom[i].duplicate(true)
		entry["id"] = "custom:%d" % i
		out.append(entry)
	var defaults := NetClient.default_classes()
	for i in defaults.size():
		var entry: Dictionary = defaults[i].duplicate(true)
		entry["id"] = "default:%d" % i
		out.append(entry)
	return out


## The class with this id, or the first default if there is none.
static func by_id(id: String) -> Dictionary:
	for entry in all():
		if entry.id == id:
			return entry
	var first: Dictionary = NetClient.default_classes()[0].duplicate(true)
	first["id"] = "default:0"
	return first


static func selected_id() -> String:
	_load()
	return _selected


## The class to spawn with, as `usable` gives it.
static func selected() -> Dictionary:
	return usable(selected_id())


## A class as it can be played now: a default as it is; a custom class
## with anything still locked taken out (`ProgressStore.unlocked_class`),
## or the first default before Create-a-Class opens.
static func usable(id: String) -> Dictionary:
	if not id.begins_with("custom"):
		return by_id(id)
	if not ProgressStore.can_create_classes():
		return by_id("default:0")
	var entry := ProgressStore.unlocked_class(by_id(id), NetClient.class_items())
	if String(NetClient.check_class(entry).error) != "":
		return by_id("default:0")
	return entry


static func select(id: String) -> void:
	_load()
	_selected = id
	save()


## Stores a custom class (its name cut to `NAME_CHARS`).
static func set_custom(slot: int, value: Dictionary) -> void:
	_load()
	if slot < 0 or slot >= _custom.size():
		return
	var entry := _clean(value)
	var name := String(value.get("name", "")).strip_edges().left(NAME_CHARS)
	entry["name"] = name if name != "" else "Custom %d" % (slot + 1)
	_custom[slot] = entry
	save()


static func use_file(file: String) -> void:
	Engine.set_meta(PATH_META, file)
	reload()


## The file in use: the one `use_file` set, or the player's. Headless runs
## (tests, dedicated servers) default to a file of their own, so a test
## that plays a match never adds to a player's.
static func path() -> String:
	var fallback := PATH if DisplayServer.get_name() != "headless" else HEADLESS_PATH
	return String(Engine.get_meta(PATH_META, fallback))


static func save() -> void:
	var config := ConfigFile.new()
	for i in _custom.size():
		config.set_value("custom", str(i), _custom[i])
	config.set_value("class", "selected", _selected)
	var err := config.save(path())
	if err != OK:
		push_warning("Could not save %s (%s)." % [path(), error_string(err)])


## Forgets what was loaded, so the next use reads `path` again (tests).
static func reload() -> void:
	_loaded = false
	_custom.clear()


static func _load() -> void:
	if _loaded:
		return
	_loaded = true
	var config := ConfigFile.new()
	var err := config.load(path())
	var defaults := NetClient.default_classes()
	_custom.clear()
	for i in CUSTOM_SLOTS:
		var stored: Variant = config.get_value("custom", str(i), null) if err == OK else null
		var entry: Dictionary
		# A saved class that no longer passes (a weapon was removed, rules
		# changed) goes back to its default.
		if stored is Dictionary and String(NetClient.check_class(stored).error) == "":
			entry = _clean(stored)
			var name := String((stored as Dictionary).get("name", "")).left(NAME_CHARS)
			entry["name"] = name if name != "" else "Custom %d" % (i + 1)
		else:
			entry = _clean(defaults[i % defaults.size()])
			entry["name"] = "Custom %d" % (i + 1)
		_custom.append(entry)
	_selected = String(config.get_value("class", "selected", "default:0")) if err == OK else ""
	if not _valid_id(_selected):
		_selected = "default:0"


static func _valid_id(id: String) -> bool:
	var kind := id.get_slice(":", 0)
	var index := int(id.get_slice(":", 1))
	if kind == "custom":
		return index >= 0 and index < CUSTOM_SLOTS
	if kind == "default":
		return index >= 0 and index < NetClient.default_classes().size()
	return false


## Only the class's own keys, with arrays copied.
static func _clean(value: Dictionary) -> Dictionary:
	var out := {}
	for key: String in [
		"primary",
		"primary_attachments",
		"secondary",
		"secondary_attachments",
		"lethals",
		"tactical",
		"tacticals",
		"perks",
		"wildcards",
	]:
		var item: Variant = value.get(key)
		if item == null:
			continue
		if item is PackedStringArray:
			item = Array(item)
		elif item is Array:
			item = (item as Array).duplicate()
		out[key] = item
	return out
