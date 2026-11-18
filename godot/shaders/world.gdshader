// Surface shader for level geometry: PBR texture sets projected in world space,
// so blockout boxes and modelled props need no UVs and textures never stretch
// when a box is resized.
//
// Cost is kept close to the old flat blockout shader: each pixel samples its
// textures once, from the one axis its normal faces most (not three blended
// projections). Large-scale brightness variation (one fetch from a small
// tiling noise texture) and grime toward the ground break up tiling.
//
// Materials live in godot/materials/world/ and are written by
// tools/build_textures.py.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_lambert, specular_schlick_ggx;

// 1 compiles in the close-up detail (High). The
// graphics preset rewrites this line (GraphicsQuality.set_world_detail); keep
// its form.
#define WORLD_DETAIL 0
// 1 compiles in wet surfaces and puddles, for maps that are wet
// (GraphicsQuality.set_world_wet, as a map loads).
#define WORLD_WET 0
#include "res://shaders/world.gdshaderinc"
