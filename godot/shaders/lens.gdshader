// The lens: one full-screen pass over the finished 3D picture, under the HUD.
// It sharpens (contrast-adaptive, so flat areas and noise are left alone),
// flares the sun, darkens the corners, and adds a little film grain. The
// vignette deepens while aiming; rounds passing close close it in and blur the
// edges for a moment; death fades the picture down. Five reads of the screen
// and a handful of arithmetic per pixel (scripts/lens.gd; measured in
// docs/performance.md).
shader_type canvas_item;
render_mode unshaded, blend_disabled;

#include "res://shaders/lens_full.gdshaderinc"
