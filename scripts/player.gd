extends CharacterBody2D
## Placeholder player. Replace the polygon with art as gameplay develops.

@export var speed: float = 300.0
const ARENA_MARGIN: float = 28.0


func _physics_process(_delta: float) -> void:
	var direction := Input.get_vector("ui_left", "ui_right", "ui_up", "ui_down")
	# Physical key checks keep WASD available without changing editor UI actions.
	var wasd := Vector2(
		float(Input.is_physical_key_pressed(KEY_D)) - float(Input.is_physical_key_pressed(KEY_A)),
		float(Input.is_physical_key_pressed(KEY_S)) - float(Input.is_physical_key_pressed(KEY_W))
	)
	velocity = (direction + wasd).limit_length() * speed
	move_and_slide()
	var arena_size := get_viewport_rect().size
	position = position.clamp(Vector2.ONE * ARENA_MARGIN, arena_size - Vector2.ONE * ARENA_MARGIN)
	if position.distance_to(get_global_mouse_position()) > 1.0:
		rotation = (get_global_mouse_position() - position).angle()
