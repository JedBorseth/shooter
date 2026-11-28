// World surfaces that need the shader's extras (WORLD_EXTRAS in
// world.gdshaderinc): stripes (awnings, road markings), checkerboards, road
// dust on vehicle paint, and projection in a model's own space (map models
// from tools/build_models.py). Every other surface uses world.gdshader, which
// compiles them out: a branch on a uniform still slows every pixel.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_lambert, specular_schlick_ggx;

#define WORLD_EXTRAS
// The graphics preset rewrites these lines (GraphicsQuality); keep their form.
#define WORLD_DETAIL 0
#define WORLD_WET 0
#include "res://shaders/world.gdshaderinc"
