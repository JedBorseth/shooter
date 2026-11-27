// Lit things in the generated maps' meshes (lamps, screens, signals): one
// shared material whose colour comes from the vertex colour and whose
// brightness is the vertex alpha times 16 (tools/build_slamcity_city.py). Bright enough to
// pass the glow threshold, and baked into the lightmaps as light sources.
shader_type spatial;
render_mode blend_mix, depth_draw_opaque, cull_back, diffuse_burley, specular_schlick_ggx;

void fragment() {
	ALBEDO = COLOR.rgb * 0.25;
	ROUGHNESS = 0.35;
	EMISSION = COLOR.rgb * COLOR.a * 16.0;
}
