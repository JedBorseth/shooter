extends Node
## Autoload `UiThemeLoader`: merges the game theme (`ui_theme.gd`) into Godot's
## default theme before any scene loads. Every Control falls back to the
## default theme, so menus, overlays under a CanvasLayer, and popups all share
## one look without setting a `theme` property anywhere.

const UiTheme := preload("res://scripts/ui/ui_theme.gd")


func _enter_tree() -> void:
	ThemeDB.get_default_theme().merge_with(UiTheme.build())
