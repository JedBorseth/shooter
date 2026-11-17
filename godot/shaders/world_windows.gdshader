// World surfaces with a grid of windows and rooms behind them (the skyline):
// see world.gdshader and world.gdshaderinc.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_lambert, specular_schlick_ggx;

#define WORLD_WINDOWS
#include "res://shaders/world.gdshaderinc"
