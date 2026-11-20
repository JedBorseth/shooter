class_name ProgressStore
extends RefCounted
## The player's progress, saved in `user://progress.cfg`: total XP (score
## is XP, plus each match's bonus) and kills with each weapon, which unlock
## its attachments. The rules (the level curve, what each level unlocks) are
## `Progression`, from `rust/sim/src/progression.rs`.
##
## Kept on this machine until accounts exist (Phase 5); the server checks
## only the class rules, not unlocks.

const PATH := "user://progress.cfg"
const HEADLESS_PATH := "user://headless_progress.cfg"

## Tests and captures point the store at a scratch file with
## `use_file`, so they never touch the player's. Kept as engine metadata,
## which a reloaded script does not reset.
const PATH_META := &"progress_store_path"

static var _xp := 0
static var _weapon_kills := {}
static var _loaded := false


static func xp() -> int:
	_load()
	return _xp


static func level() -> int:
	return int(Progression.rank(xp()).level)


## Kills with a weapon (its plain key, "m4").
static func weapon_kills(weapon: String) -> int:
	_load()
	return int(_weapon_kills.get(weapon, 0))


## Adds XP and weapon kills (plain weapon key to count) and saves.
static func add(xp_gained: int, kills: Dictionary = {}) -> void:
	_load()
	_xp = maxi(_xp + maxi(xp_gained, 0), 0)
	for weapon: String in kills:
		_weapon_kills[weapon] = weapon_kills(weapon) + int(kills[weapon])
	save()


static func can_create_classes() -> bool:
	return level() >= Progression.create_a_class_level()


## "" when the item is unlocked, otherwise what unlocks it ("Level 12").
## `kind` is "weapon", "tactical", or "wildcard".
static func lock_reason(kind: String, key: String) -> String:
	var needed := Progression.unlock_level(kind, key)
	return "" if level() >= needed else "Level %d" % needed


## "" when the attachment is unlocked on the weapon, otherwise how many
## kills with it are still needed ("12 kills").
static func attachment_lock(weapon: String, attachment: String) -> String:
	var needed := Progression.attachment_kills(attachment)
	var have := weapon_kills(weapon)
	return "" if have >= needed else "%d kills" % (needed - have)


## A custom class with everything still locked taken out: a locked weapon
## becomes the first unlocked one of its slot, and locked attachments,
## tacticals, and wildcards come off. Default classes are never filtered.
static func unlocked_class(entry: Dictionary, items: Dictionary) -> Dictionary:
	var out := entry.duplicate(true)
	for slot: Array in [
		["primary", "primary_attachments", "primaries"],
		["secondary", "secondary_attachments", "secondaries"]
	]:
		var weapon := String(out.get(slot[0], ""))
		if weapon != "" and lock_reason("weapon", weapon) != "":
			# With Two Primaries, the secondary slot may hold a primary.
			var list: Array = items.primaries if _in(items.primaries, weapon) else items[slot[2]]
			weapon = _first_unlocked(list)
			out[slot[0]] = weapon
		var kept := []
		for attachment: String in Array(out.get(slot[1], [])):
			if weapon != "" and attachment_lock(weapon, attachment) == "":
				kept.append(attachment)
		out[slot[1]] = kept
	if lock_reason("tactical", String(out.get("tactical", "flash"))) != "":
		out["tactical"] = "flash"
	var wildcards := []
	for wildcard: String in Array(out.get("wildcards", [])):
		if lock_reason("wildcard", wildcard) == "":
			wildcards.append(wildcard)
	out["wildcards"] = wildcards
	# Without its wildcard, a second frag goes too.
	if not "danger_close" in wildcards:
		out["lethals"] = mini(int(out.get("lethals", 0)), 1)
	return out


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
	config.set_value("progress", "xp", _xp)
	config.set_value("progress", "weapon_kills", _weapon_kills)
	var err := config.save(path())
	if err != OK:
		push_warning("Could not save %s (%s)." % [path(), error_string(err)])


## Forgets what was loaded, so the next use reads `path` again (tests).
static func reload() -> void:
	_loaded = false


static func _load() -> void:
	if _loaded:
		return
	_loaded = true
	_xp = 0
	_weapon_kills = {}
	var config := ConfigFile.new()
	if config.load(path()) != OK:
		return
	var stored_xp: Variant = config.get_value("progress", "xp", 0)
	_xp = maxi(int(stored_xp), 0) if stored_xp is int else 0
	var kills: Variant = config.get_value("progress", "weapon_kills", {})
	if kills is Dictionary:
		for key: Variant in kills:
			var count: Variant = kills[key]
			if key is String and count is int:
				_weapon_kills[key] = maxi(count, 0)


static func _in(list: Array, key: String) -> bool:
	return list.any(func(item: Dictionary) -> bool: return item.key == key)


static func _first_unlocked(list: Array) -> String:
	for item: Dictionary in list:
		if lock_reason("weapon", String(item.key)) == "":
			return String(item.key)
	return ""
