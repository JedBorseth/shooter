// The lens on the Medium preset: the vignette, the grain, and the sun flare of
// lens.gdshader without reading the screen, at about a quarter of its cost. It
// blends with premultiplied alpha, so one pass can both darken (the vignette,
// dark specks) and add light (the flare, light specks). No sharpening, and no
// suppression blur.
shader_type canvas_item;
render_mode unshaded, blend_premul_alpha;

#include "res://shaders/lens_lite.gdshaderinc"
