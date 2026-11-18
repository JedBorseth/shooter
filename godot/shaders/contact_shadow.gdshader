// A contact shadow: a soft dark patch on the ground under a character's feet
// (scripts/grounding.gd lays it on the ground, shapes and turns it by the
// body's stance, and fades it as the feet leave the ground). A flat quad that
// multiplies what is under it, so it costs only the few pixels it covers: no
// depth read (which would copy the depth buffer every frame) and no Decal
// (which turns on decal shading for every pixel of a map that has none).
shader_type spatial;
render_mode unshaded, blend_mul, depth_draw_never, cull_disabled, shadows_disabled, fog_disabled;

// Distance from the camera where the patch starts to fade, and how long it takes.
uniform float fade_begin = 35.0;
uniform float fade_length = 10.0;
// In direct sun the patch keeps this share of its darkness: the sun's own
// shadow grounds the feet there. Past the sun's shadow distance there is no
// sun shadow, so the patch is full again.
uniform float sun_keep = 0.4;
uniform float sun_shadow_end = 100.0;
// 1 with the feet on the ground, falling to 0 as they rise off it (a jump).
instance uniform float shown = 1.0;
// Darkness at the centre for the stance: the ground keeps 1 - darkness.
instance uniform float darkness = 0.6;
// 1 in direct sun, 0 in shade.
instance uniform float sunlit = 0.0;

void fragment() {
	// The quad is scaled to the stance, so this is an oval on the ground.
	float r = length(UV * 2.0 - 1.0);
	// A full-strength core under the feet that falls off smoothly to nothing.
	float shape = 1.0 - smoothstep(0.2, 1.0, r);
	float far = length(NODE_POSITION_WORLD - CAMERA_POSITION_WORLD);
	float fade = 1.0 - smoothstep(fade_begin, fade_begin + fade_length, far);
	float shadowed = 1.0 - smoothstep(sun_shadow_end * 0.8, sun_shadow_end, far);
	// In sun, the sharper core stays and the wide skirt goes.
	float sun = sunlit * shadowed;
	float core = 1.0 - smoothstep(0.0, 0.6, r);
	float strength = darkness * mix(shape, core * sun_keep, sun);
	ALBEDO = vec3(1.0 - strength * shown * fade);
}
