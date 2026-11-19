// The sniper scope's lens, drawn over the picture while scoped
// (scripts/scope_overlay.gd). It multiplies the picture, so it never reads the
// screen: black outside the tube, a soft eye-relief shadow inside the rim that
// slides toward one side as the scope swings away from the eye, a faint
// coloured fringe at the rim, a gentle darkening toward the edge of the
// glass, and the faint green-amber cast and light loss of coated glass. The
// reticle is drawn on top as crisp lines. While the scope comes up to the
// eye, the lens follows the lens the weapon cuts round its eyepiece
// (`viewmodel_scope_lens`, set by the viewmodel rig) and the surround fades
// in (`open`), so the picture grows out of the eyepiece instead of cutting in.
shader_type canvas_item;
render_mode unshaded, blend_mul;

// Radius of the lens, as a fraction of the screen height.
uniform float radius = 0.46;
// Where the eye sits relative to the lens centre, as a fraction of the radius:
// the shadow is thickest on the opposite side.
uniform vec2 eye = vec2(0.0);
// Width of the eye-relief shadow, as a fraction of the radius.
uniform float shadow_width = 0.16;
// What the glass passes of each colour: a riflescope's coatings pass about
// nine tenths of the light, a little less blue than green.
uniform vec3 glass_tint = vec3(0.93, 0.95, 0.88);
// How far the scope has come up: 0 draws nothing, 1 the full scope.
uniform float open : hint_range(0.0, 1.0) = 1.0;
// Twice the tangent of half the first-person weapon's vertical field of view
// (55 degrees): turns the lens's directions into screen heights.
uniform float viewmodel_span = 1.041134;
// The lens the first-person weapon cuts round a raised scope's eyepiece
// (centre and radius as directions from the eye); zero once the weapon hides.
global uniform vec4 viewmodel_scope_lens;

void fragment() {
	vec2 size = 1.0 / SCREEN_PIXEL_SIZE;
	vec2 centre = size * 0.5;
	float lens_radius = radius;
	if (viewmodel_scope_lens.w > 0.0) {
		// Screen y grows down; the view's y grows up.
		centre += vec2(viewmodel_scope_lens.x, -viewmodel_scope_lens.y) / viewmodel_span * size.y;
		lens_radius = viewmodel_scope_lens.w / viewmodel_span;
	}
	vec2 p = (FRAGCOORD.xy - centre) / (size.y * lens_radius);
	float d = length(p);
	// Antialiased edge of the tube, about a pixel and a half wide.
	float px = 1.5 / (size.y * lens_radius);
	float glass = 1.0 - smoothstep(1.0 - px, 1.0, d);
	// Eye-relief shadow: a soft ring measured from the eye's centre, so it
	// grows on the side the scope has swung away from.
	float from_eye = length(p - eye * 0.35);
	float shadow = smoothstep(1.0 - shadow_width - length(eye) * 0.15, 1.02, from_eye);
	vec3 colour = vec3(1.0 - shadow * 0.92);
	// The glass darkens a little toward its edge.
	colour *= 1.0 - 0.18 * d * d;
	colour *= glass_tint;
	// Chromatic fringe just inside the rim: blue on the inside, warm on the outside.
	float rim = smoothstep(0.86, 0.95, d) * (1.0 - smoothstep(0.95, 1.0, d));
	colour *= mix(vec3(1.0), vec3(0.82, 0.9, 1.0), smoothstep(0.86, 0.93, d) * (1.0 - smoothstep(0.93, 0.97, d)));
	colour *= mix(vec3(1.0), vec3(1.0, 0.86, 0.72), rim * smoothstep(0.95, 0.98, d));
	COLOR = vec4(mix(vec3(1.0), colour * glass, open), 1.0);
}
