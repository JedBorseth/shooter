// World surfaces with a grid of windows and rooms behind them (the skyline):
// see world.gdshader and world.gdshaderinc.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_lambert, specular_schlick_ggx;

#define WORLD_WINDOWS
// 1 compiles in the close-up detail (High). The
// graphics preset rewrites this line (GraphicsQuality.set_world_detail); keep
// its form.
#define WORLD_DETAIL 0
// 1 compiles in wet surfaces and puddles, for maps that are wet
// (GraphicsQuality.set_world_wet, as a map loads).
#define WORLD_WET 0
#include "res://shaders/world.gdshaderinc"
