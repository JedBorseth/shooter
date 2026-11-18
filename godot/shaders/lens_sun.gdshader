// lens.gdshader with the sun flare (shaders/lens_flare.gdshaderinc), drawn
// while a map's sun is in view: the flare costs a little on every pixel, so
// scripts/lens.gd switches to this only when there is a flare to draw.
shader_type canvas_item;
render_mode unshaded, blend_disabled;

#define LENS_SUN
#include "res://shaders/lens_full.gdshaderinc"
