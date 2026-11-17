extends StaticBody3D
## Both local hitscan and the server identify the owning destructible piece.

var manager: Node
var prop_id := 0


func apply_damage(_amount: float, _headshot: bool) -> bool:
	# The player's prop path supplies a direction separately from character damage.
	return false


func apply_prop_hit(amount: float, direction: Vector3) -> void:
	manager.damage_prop(prop_id, amount, direction * amount, -1, 0)
