// The body of lens.gdshader and lens_sun.gdshader (see there).
#ifdef LENS_SUN
#include "res://shaders/lens_flare.gdshaderinc"
#endif

uniform sampler2D screen_texture : hint_screen_texture, repeat_disable, filter_nearest;
// 0 leaves the picture as it is; 1 is the strongest sharpening.
uniform float sharpen : hint_range(0.0, 1.0) = 0.45;
// How far the corners darken, as a fraction of their brightness.
uniform float vignette : hint_range(0.0, 1.0) = 0.13;
// Film grain strength, as a fraction of full brightness.
uniform float grain : hint_range(0.0, 0.2) = 0.014;
// 0 to 1 while aiming down sights: the edges darken a little more.
uniform float aim : hint_range(0.0, 1.0) = 0.0;
// 0 to 1 just after rounds pass close: the edges close in.
uniform float suppression : hint_range(0.0, 1.0) = 0.0;
// 0 to 1 after death: the picture fades down.
uniform float dead : hint_range(0.0, 1.0) = 0.0;
// Changes every frame, so the grain does not sit still.
uniform float tick = 0.0;

float hash(vec2 p) {
	vec3 p3 = fract(vec3(p.xyx) * 0.1031);
	p3 += dot(p3, p3.yzx + 33.33);
	return fract((p3.x + p3.y) * p3.z);
}

void fragment() {
	vec2 px = SCREEN_PIXEL_SIZE;
	vec3 centre = texture(screen_texture, SCREEN_UV).rgb;
	vec3 north = texture(screen_texture, SCREEN_UV + vec2(0.0, -px.y)).rgb;
	vec3 south = texture(screen_texture, SCREEN_UV + vec2(0.0, px.y)).rgb;
	vec3 west = texture(screen_texture, SCREEN_UV + vec2(-px.x, 0.0)).rgb;
	vec3 east = texture(screen_texture, SCREEN_UV + vec2(px.x, 0.0)).rgb;

	// Contrast-adaptive sharpening (after AMD's FidelityFX CAS): the weight of
	// the neighbours shrinks where the local contrast is already high, so
	// edges do not ring and antialiased steps stay smooth.
	vec3 low = min(centre, min(min(north, south), min(west, east)));
	vec3 high = max(centre, max(max(north, south), max(west, east)));
	vec3 room = sqrt(clamp(min(low, 1.0 - high) / max(high, vec3(0.0001)), 0.0, 1.0));
	vec3 weight = room * (-1.0 / mix(9.0, 5.0, sharpen));
	vec3 colour = (centre + (north + south + west + east) * weight) / (1.0 + 4.0 * weight);
	colour = mix(centre, colour, step(0.001, sharpen));

	vec2 centred = (SCREEN_UV - 0.5) * vec2(1.0, 0.82);
	float edge = smoothstep(0.45, 0.95, length(centred) * 1.55);
	// Suppression blurs the edges for a moment: four reads toward the middle,
	// only while it lasts.
	if (suppression > 0.01) {
		vec2 step_uv = (0.5 - SCREEN_UV) * 0.012 * suppression * edge;
		vec3 blur = texture(screen_texture, SCREEN_UV + step_uv).rgb
			+ texture(screen_texture, SCREEN_UV + step_uv * 2.0).rgb
			+ texture(screen_texture, SCREEN_UV - step_uv).rgb
			+ texture(screen_texture, SCREEN_UV + step_uv * 3.0).rgb;
		colour = mix(colour, blur * 0.25, clamp(edge * suppression * 1.5, 0.0, 1.0));
	}

	float aspect = SCREEN_PIXEL_SIZE.y / SCREEN_PIXEL_SIZE.x;
#ifdef LENS_SUN
	// Screen, not add: the flare brightens toward white without clipping.
	vec3 sun_light = clamp(lens_flare(SCREEN_UV, aspect), 0.0, 1.0);
	colour = 1.0 - (1.0 - colour) * (1.0 - sun_light);
#endif

	// Vignette: wider than tall, so the corners go first.
	float shade = vignette + aim * 0.16 + suppression * 0.45 + dead * 0.4;
	colour *= 1.0 - edge * clamp(shade, 0.0, 0.9);
	colour *= 1.0 - dead * 0.35;

	// Grain: strongest in the mid-tones, gone in black and in white.
	float luma = clamp(dot(colour, vec3(0.2126, 0.7152, 0.0722)), 0.0, 1.0);
	// Grain two pixels wide reads as film; single pixels read as sensor noise.
	float noise = hash(floor(FRAGCOORD.xy * 0.5) + vec2(tick * 37.0, tick * 91.0)) - 0.5;
	colour += noise * grain * (0.25 + 3.0 * luma * (1.0 - luma));

	COLOR = vec4(colour, 1.0);
}
