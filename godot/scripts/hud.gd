extends CanvasLayer
## Heads-up display: crosshair, hit markers, ammo, and a movement readout for tuning.

const GAMEPAD_CONTROLS := (
	"LS move   L3 sprint (toggle)   B crouch (slide while sprinting)   A jump"
	+ "   RT fire   LT aim   X reload   Y switch   RS look (aim assist on)"
)

@export var player_path: NodePath

@onready var _player: FpsPlayer = get_node_or_null(player_path) as FpsPlayer
@onready var _crosshair: Control = $Crosshair
@onready var _readout: Label = $Readout
@onready var _weapon: Label = $Ammo/Weapon
@onready var _ammo: Label = $Ammo/Count
@onready var _controls: Label = $Controls
@onready var _keyboard_controls := _controls.text


func _ready() -> void:
	if _player != null:
		_player.hit_confirmed.connect(_crosshair.show_hit)


func _process(_delta: float) -> void:
	if _player == null:
		return
	_readout.text = (
		"%s   %.1f m/s   %d fps"
		% [
			_player.movement_state(),
			_player.horizontal_speed(),
			Engine.get_frames_per_second(),
		]
	)
	_controls.text = GAMEPAD_CONTROLS if _player.using_gamepad() else _keyboard_controls
	_weapon.text = _player.weapon_name()
	_ammo.text = (
		"RELOADING"
		if _player.is_reloading()
		else "%d / %d" % [_player.magazine(), _player.reserve()]
	)
	_ammo.modulate = Color(1, 0.35, 0.3) if _player.magazine() == 0 else Color.WHITE

	# Convert the spread cone to pixels so the crosshair shows where shots can land.
	var half_fov := deg_to_rad(_player.camera_fov()) * 0.5
	var pixels_per_unit := get_viewport().get_visible_rect().size.y * 0.5 / tan(half_fov)
	_crosshair.gap = tan(_player.spread_radians()) * pixels_per_unit
	_crosshair.alpha = 1.0 - clampf(_player.ads_amount() * 1.5, 0.0, 1.0)
