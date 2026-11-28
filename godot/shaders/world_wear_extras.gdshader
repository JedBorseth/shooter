// world_wear.gdshader with the extras as well (WORLD_EXTRAS): the hero
// models' vehicle paint keeps its road dust under the baked wear.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_lambert, specular_schlick_ggx;

// The graphics preset rewrites these lines, as in world.gdshader; keep their form.
#define WORLD_DETAIL 0
#define WORLD_WET 0
#define BAKED_WEAR
#define WORLD_EXTRAS
#include "res://shaders/world.gdshaderinc"
