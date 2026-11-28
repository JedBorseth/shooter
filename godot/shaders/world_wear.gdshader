// The world surface shader with baked wear (BAKED_WEAR in
// world.gdshaderinc): the hero models (tanks, tankers, pumps, the ship) whose
// UVs index the wear mask atlas that tools/world_wear.py bakes. Every other
// surface uses world.gdshader and skips that code.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_lambert, specular_schlick_ggx;

// The graphics preset rewrites these lines, as in world.gdshader; keep their form.
#define WORLD_DETAIL 0
#define WORLD_WET 0
#define BAKED_WEAR
#include "res://shaders/world.gdshaderinc"
