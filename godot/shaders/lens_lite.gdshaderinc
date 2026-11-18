// The body of lens_lite.gdshader and lens_lite_sun.gdshader (see there).
#ifdef LENS_SUN
#include "res://shaders/lens_flare.gdshaderinc"
#endif

uniform float vignette : hint_range(0.0, 1.0) = 0.13;
uniform float grain : hint_range(0.0, 0.2) = 0.014;
uniform float aim : hint_range(0.0, 1.0) = 0.0;
uniform float suppression : hint_range(0.0, 1.0) = 0.0;
uniform float dead : hint_range(0.0, 1.0) = 0.0;
uniform float tick = 0.0;

float hash(vec2 p) {
	vec3 p3 = fract(vec3(p.xyx) * 0.1031);
	p3 += dot(p3, p3.yzx + 33.33);
	return fract((p3.x + p3.y) * p3.z);
}

void fragment() {
	vec2 centred = (SCREEN_UV - 0.5) * vec2(1.0, 0.82);
	float edge = smoothstep(0.45, 0.95, length(centred) * 1.55);
	float shade = edge * clamp(vignette + aim * 0.16 + suppression * 0.7 + dead * 0.4, 0.0, 0.9);
	shade = 1.0 - (1.0 - shade) * (1.0 - dead * 0.35);
	// Grain two pixels wide reads as film; single pixels read as sensor noise.
	float noise = hash(floor(FRAGCOORD.xy * 0.5) + vec2(tick * 37.0, tick * 91.0)) - 0.5;
	// A speck is white or black; how strongly it shows is the grain.
	float speck = abs(noise) * 2.0 * grain;
	float alpha = shade + speck * (1.0 - shade);
	// Premultiplied: light specks add, dark specks only cover.
	vec3 colour = vec3(step(0.0, noise)) * speck * (1.0 - shade);
#ifdef LENS_SUN
	colour += lens_flare(SCREEN_UV, SCREEN_PIXEL_SIZE.y / SCREEN_PIXEL_SIZE.x) * (1.0 - shade);
#endif
	COLOR = vec4(colour, alpha);
}
