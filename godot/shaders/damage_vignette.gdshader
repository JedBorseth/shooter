// Screen-edge vignette for low health and fresh hits. It multiplies the
// picture toward a dark blood red at the edges, so the middle of the screen
// (where the fight is) stays clear and the sky does not turn pink. The edge is
// not a clean ring: it is broken into smears running in from the rim, like
// blood on the lens, and a fresh hit darkens it toward black for a moment.
// scripts/damage_overlay.gd hides it entirely at full health, so it costs
// nothing then.
shader_type canvas_item;
render_mode blend_mul, unshaded;

uniform float intensity : hint_range(0.0, 1.0) = 0.0;
// A fresh hit, fading in a fraction of a second: darker and further in.
uniform float hit : hint_range(0.0, 1.0) = 0.0;
// What the edges are multiplied toward at full intensity.
uniform vec4 tint : source_color = vec4(0.55, 0.06, 0.04, 1.0);
// Changes per life, so the smears are not always in the same places.
uniform float seed = 0.0;

float hash(float n) {
	return fract(sin(n * 12.9898 + seed * 78.233) * 43758.5453);
}

// Smooth noise around the rim: `a` is the angle in turns, 0 to 1.
float rim_noise(float a, float cells) {
	float x = a * cells;
	float i = floor(x);
	float f = fract(x);
	f = f * f * (3.0 - 2.0 * f);
	// Wrap the last cell onto the first, so there is no seam at the top.
	return mix(hash(mod(i, cells)), hash(mod(i + 1.0, cells)), f);
}

void fragment() {
	vec2 centred = UV * 2.0 - 1.0;
	// Wider than tall, so the corners and sides darken first.
	float edge = length(centred * vec2(0.85, 1.0));
	float turn = atan(centred.y, centred.x) / 6.28318 + 0.5;
	// Smears: some reach far in, some barely past the rim.
	float smear = rim_noise(turn, 23.0) * 0.6 + rim_noise(turn, 61.0) * 0.4;
	float reach = intensity * 0.6 + hit * 0.25;
	// The corners sit at about 1.31, so the ramp must end there to reach full
	// strength at all.
	float start = 1.05 - reach * mix(0.55, 1.25, smear);
	float mask = smoothstep(start, 1.3, edge);
	// Reaches full strength under half intensity, so a hurt player sees it
	// well before the last sliver of health.
	float amount = clamp(intensity * 2.2 + hit, 0.0, 1.0);
	// Thicker smears are darker: dried blood, not a red filter.
	vec3 colour = tint.rgb * mix(1.0, 0.45, smoothstep(0.55, 0.9, smear) * mask);
	colour = mix(colour, vec3(0.08, 0.0, 0.0), hit * 0.6);
	COLOR = vec4(mix(vec3(1.0), colour, mask * amount * tint.a), 1.0);
}
