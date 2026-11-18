// The sniper scope's lens, drawn over the picture while scoped
// (scripts/scope_overlay.gd). It multiplies the picture, so it never reads the
// screen: black outside the tube, a soft eye-relief shadow inside the rim that
// slides toward one side as the scope swings away from the eye, a faint
// coloured fringe at the rim, and a gentle darkening toward the edge of the
// glass. The reticle is drawn on top as crisp lines.
shader_type canvas_item;
render_mode unshaded, blend_mul;

// Radius of the lens, as a fraction of the screen height.
uniform float radius = 0.46;
// Where the eye sits relative to the lens centre, as a fraction of the radius:
// the shadow is thickest on the opposite side.
uniform vec2 eye = vec2(0.0);
// Width of the eye-relief shadow, as a fraction of the radius.
uniform float shadow_width = 0.16;

void fragment() {
	vec2 size = 1.0 / SCREEN_PIXEL_SIZE;
	vec2 p = (FRAGCOORD.xy - size * 0.5) / (size.y * radius);
	float d = length(p);
	// Antialiased edge of the tube, about a pixel and a half wide.
	float px = 1.5 / (size.y * radius);
	float glass = 1.0 - smoothstep(1.0 - px, 1.0, d);
	// Eye-relief shadow: a soft ring measured from the eye's centre, so it
	// grows on the side the scope has swung away from.
	float from_eye = length(p - eye * 0.35);
	float shadow = smoothstep(1.0 - shadow_width - length(eye) * 0.15, 1.02, from_eye);
	vec3 colour = vec3(1.0 - shadow * 0.92);
	// The glass darkens a little toward its edge.
	colour *= 1.0 - 0.18 * d * d;
	// Chromatic fringe just inside the rim: blue on the inside, warm on the outside.
	float rim = smoothstep(0.86, 0.95, d) * (1.0 - smoothstep(0.95, 1.0, d));
	colour *= mix(vec3(1.0), vec3(0.82, 0.9, 1.0), smoothstep(0.86, 0.93, d) * (1.0 - smoothstep(0.93, 0.97, d)));
	colour *= mix(vec3(1.0), vec3(1.0, 0.86, 0.72), rim * smoothstep(0.95, 0.98, d));
	COLOR = vec4(colour * glass, 1.0);
}
