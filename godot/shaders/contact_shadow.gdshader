// A contact shadow: a soft dark patch on the ground under a character's feet
// (scripts/grounding.gd lays it on the ground and fades it as the feet leave
// it). A flat quad that multiplies what is under it, so it costs only the few
// pixels it covers: no depth read (which would copy the depth buffer every
// frame) and no Decal (which turns on decal shading for every pixel of a map
// that has none).
shader_type spatial;
render_mode unshaded, blend_mul, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled;

// Darkness at the centre: the ground keeps 1 - strength of its light.
uniform float strength = 0.6;
// Distance from the camera where the patch starts to fade, and how long it takes.
uniform float fade_begin = 35.0;
uniform float fade_length = 10.0;
// 1 with the feet on the ground, falling to 0 as they rise off it (a jump).
instance uniform float shown = 1.0;

void fragment() {
	float r = length(UV * 2.0 - 1.0);
	// A full-strength core under the feet that falls off smoothly to nothing.
	float shape = 1.0 - smoothstep(0.2, 1.0, r);
	float far = length(NODE_POSITION_WORLD - CAMERA_POSITION_WORLD);
	float fade = 1.0 - smoothstep(fade_begin, fade_begin + fade_length, far);
	ALBEDO = vec3(1.0 - strength * shape * shown * fade);
}
