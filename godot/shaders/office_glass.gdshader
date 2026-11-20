// Interior office glazing (Warehouse's scanned office): an unlit, single-sided
// tint whose opacity and brightness rise toward grazing angles (fresnel), with
// faint diagonal reflection streaks in world space. Call of Duty draws interior
// glass as a cheap sheen over the room behind it rather than a fully lit
// surface; lit, double-sided panes cost about 5 ms on Medium (GTX 1660 Ti)
// when a partition fills the screen.
shader_type spatial;
render_mode unshaded, blend_mix, depth_draw_never, cull_back, fog_disabled;

uniform vec3 tint : source_color = vec3(0.36, 0.42, 0.44);
uniform vec3 sheen : source_color = vec3(0.86, 0.9, 0.92);
uniform float face_alpha = 0.07;
uniform float edge_alpha = 0.5;

void fragment() {
	float facing = clamp(dot(NORMAL, VIEW), 0.0, 1.0);
	float f = pow(1.0 - facing, 4.0);
	vec3 world = (INV_VIEW_MATRIX * vec4(VERTEX, 1.0)).xyz;
	float band = sin((world.x + world.z) * 1.7 + world.y * 0.9);
	float streak = smoothstep(0.82, 1.0, band) * 0.06;
	ALBEDO = mix(tint, sheen, clamp(f + streak * 4.0, 0.0, 1.0));
	ALPHA = clamp(mix(face_alpha, edge_alpha, f) + streak, 0.0, 1.0);
}
