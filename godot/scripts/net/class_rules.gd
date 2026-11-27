extends RefCounted
## Pick 10's edits for Create-a-Class (`class_menu.gd`): each change takes a
## class and returns it changed, keeping it as close to valid as it can
## (dropping what the new choice no longer allows), and lookups into
## `NetClient.class_items()`. The rules themselves are the server's
## (`NetClient.check_class`).

var items := {}


func _init(class_items: Dictionary) -> void:
	items = class_items


func with_primary(draft: Dictionary, key: String) -> Dictionary:
	draft["primary"] = key
	draft["primary_attachments"] = offered(key, Array(draft.get("primary_attachments", [])))
	return draft


func with_secondary(draft: Dictionary, key: String) -> Dictionary:
	draft["secondary"] = key
	draft["secondary_attachments"] = offered(key, Array(draft.get("secondary_attachments", [])))
	return draft


func with_value(draft: Dictionary, field: String, value: int) -> Dictionary:
	draft[field] = value
	return draft


func with_tactical(draft: Dictionary, key: String, count: int) -> Dictionary:
	draft["tactical"] = key
	draft["tacticals"] = count
	return draft


func toggle_attachment(draft: Dictionary, field: String, key: String) -> Dictionary:
	var fitted: Array = Array(draft.get(field, []))
	if key in fitted:
		fitted.erase(key)
	else:
		# A new optic replaces the one fitted.
		if is_optic(key):
			for other: String in fitted.duplicate():
				if is_optic(other):
					fitted.erase(other)
		fitted.append(key)
	draft[field] = fitted
	return draft


func toggle_wildcard(draft: Dictionary, key: String) -> Dictionary:
	var wildcards: Array = Array(draft.get("wildcards", []))
	if key in wildcards:
		wildcards.erase(key)
		# Drop what only the wildcard allowed.
		match key:
			"gunfighter":
				var fitted: Array = Array(draft.get("primary_attachments", []))
				draft["primary_attachments"] = fitted.slice(0, 2)
			"overkill":
				if is_primary(String(draft.get("secondary", ""))):
					draft["secondary"] = String(items.secondaries[0].key)
					draft["secondary_attachments"] = []
			"danger_close":
				draft["lethals"] = mini(int(draft.get("lethals", 0)), 1)
			"perk1_greed", "perk2_greed", "perk3_greed":
				# Keep the tier's first perk.
				var tier := int(key.substr(4, 1))
				var kept := []
				var seen := false
				for perk: String in Array(draft.get("perks", [])):
					if int(perk(perk).get("tier", 0)) == tier:
						if seen:
							continue
						seen = true
					kept.append(perk)
				draft["perks"] = kept
	else:
		wildcards.append(key)
	draft["wildcards"] = wildcards
	return draft


## Takes a perk off, or puts it on. A full slot gives up its first perk
## to the new one.
func toggle_perk(draft: Dictionary, key: String) -> Dictionary:
	var perks: Array = Array(draft.get("perks", []))
	if key in perks:
		perks.erase(key)
	else:
		var tier := int(perk(key).get("tier", 0))
		var limit := 2 if "perk%d_greed" % tier in Array(draft.get("wildcards", [])) else 1
		var in_tier := perks.filter(
			func(p: String) -> bool: return int(perk(p).get("tier", 0)) == tier
		)
		if in_tier.size() >= limit:
			perks.erase(in_tier[0])
		perks.append(key)
	draft["perks"] = perks
	return draft


func perk(key: String) -> Dictionary:
	for item: Dictionary in items.perks:
		if item.key == key:
			return item
	return {}


func weapon(key: String) -> Dictionary:
	for list: String in ["primaries", "secondaries"]:
		for entry: Dictionary in items[list]:
			if entry.key == key:
				return entry
	return {}


func is_primary(key: String) -> bool:
	for entry: Dictionary in items.primaries:
		if entry.key == key:
			return true
	return false


func is_optic(key: String) -> bool:
	for entry: Dictionary in items.primaries:
		for item: Dictionary in entry.attachments:
			if item.key == key:
				return bool(item.optic)
	return false


## The attachments of `fitted` that weapon `key` takes and has unlocked.
func offered(key: String, fitted: Array) -> Array:
	var keys := []
	for item: Dictionary in weapon(key).get("attachments", []):
		if ProgressStore.attachment_lock(key, String(item.key)) == "":
			keys.append(String(item.key))
	return fitted.filter(func(key: String) -> bool: return key in keys)


func weapon_name(key: String) -> String:
	return String(weapon(key).get("name", key))


func attachment_names(key: String, keys: Array) -> String:
	var names := []
	for item: Dictionary in weapon(key).get("attachments", []):
		if String(item.key) in Array(keys):
			names.append(String(item.name))
	return " + ".join(names)


func tactical_name(key: String) -> String:
	for item: Dictionary in items.tacticals:
		if item.key == key:
			return String(item.name)
	return key


func wildcard_name(key: String) -> String:
	for item: Dictionary in items.wildcards:
		if item.key == key:
			return String(item.name)
	return key
