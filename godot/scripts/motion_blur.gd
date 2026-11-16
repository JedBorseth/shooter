extends CompositorEffect
## Motion blur, as a compositor effect: each pixel averages a few samples of the
## frame along its motion vector (the renderer's velocity buffer), so fast
## turns and fast-moving things smear a little. Off unless the player turns it
## on in the settings; `GraphicsQuality` adds it to every map's environment.

const SHADER := """
#version 450
layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;
layout(set = 0, binding = 0) uniform sampler2D source_color;
layout(set = 0, binding = 1) uniform sampler2D velocity_buffer;
layout(rgba16f, set = 0, binding = 2) uniform restrict writeonly image2D output_image;
layout(push_constant, std430) uniform Params {
	vec2 size;
	float intensity;
	float samples;
} params;

void main() {
	ivec2 pixel = ivec2(gl_GlobalInvocationID.xy);
	if (pixel.x >= int(params.size.x) || pixel.y >= int(params.size.y)) {
		return;
	}
	vec2 uv = (vec2(pixel) + 0.5) / params.size;
	vec2 velocity = texture(velocity_buffer, uv).xy * params.intensity;
	float length_px = length(velocity * params.size);
	if (length_px < 0.75) {
		imageStore(output_image, pixel, texture(source_color, uv));
		return;
	}
	// Cap the streak, so a teleport or a camera cut does not smear the frame.
	float max_px = 48.0;
	if (length_px > max_px) {
		velocity *= max_px / length_px;
	}
	int count = int(params.samples);
	vec4 sum = vec4(0.0);
	for (int i = 0; i < count; i++) {
		float t = float(i) / float(count - 1) - 0.5;
		sum += texture(source_color, uv + velocity * t);
	}
	imageStore(output_image, pixel, sum / float(count));
}
"""

## The renderer's colour buffer cannot be a copy source, so a first pass copies
## it into our own texture with image loads, and the blur reads that copy.
const COPY_SHADER := """
#version 450
layout(local_size_x = 8, local_size_y = 8, local_size_z = 1) in;
layout(rgba16f, set = 0, binding = 0) uniform restrict readonly image2D source_image;
layout(rgba16f, set = 0, binding = 1) uniform restrict writeonly image2D copy_image;
void main() {
	ivec2 pixel = ivec2(gl_GlobalInvocationID.xy);
	ivec2 size = imageSize(source_image);
	if (pixel.x >= size.x || pixel.y >= size.y) {
		return;
	}
	imageStore(copy_image, pixel, imageLoad(source_image, pixel));
}
"""

## How much of the frame's motion becomes blur. 1.0 is a full frame's movement.
@export_range(0.0, 2.0) var intensity := 0.5
@export_range(4, 16) var samples := 8

var _rd: RenderingDevice
var _shader: RID
var _pipeline: RID
var _copy_shader: RID
var _copy_pipeline: RID
var _sampler: RID
## Our own copy of the frame, recreated when the render size changes.
var _copy: RID
var _copy_size := Vector2i.ZERO


func _init() -> void:
	effect_callback_type = EFFECT_CALLBACK_TYPE_POST_TRANSPARENT
	needs_motion_vectors = true
	RenderingServer.call_on_render_thread(_initialize_compute)


func _notification(what: int) -> void:
	if what == NOTIFICATION_PREDELETE and _rd != null:
		if _shader.is_valid():
			_rd.free_rid(_shader)
		if _copy_shader.is_valid():
			_rd.free_rid(_copy_shader)
		if _sampler.is_valid():
			_rd.free_rid(_sampler)
		if _copy.is_valid():
			_rd.free_rid(_copy)


func _initialize_compute() -> void:
	_rd = RenderingServer.get_rendering_device()
	if _rd == null:
		return
	var source := RDShaderSource.new()
	source.language = RenderingDevice.SHADER_LANGUAGE_GLSL
	source.source_compute = SHADER
	var spirv := _rd.shader_compile_spirv_from_source(source)
	if not spirv.compile_error_compute.is_empty():
		push_error("motion blur: %s" % spirv.compile_error_compute)
		return
	_shader = _rd.shader_create_from_spirv(spirv)
	_pipeline = _rd.compute_pipeline_create(_shader)
	var copy_source := RDShaderSource.new()
	copy_source.language = RenderingDevice.SHADER_LANGUAGE_GLSL
	copy_source.source_compute = COPY_SHADER
	var copy_spirv := _rd.shader_compile_spirv_from_source(copy_source)
	if not copy_spirv.compile_error_compute.is_empty():
		push_error("motion blur: %s" % copy_spirv.compile_error_compute)
		return
	_copy_shader = _rd.shader_create_from_spirv(copy_spirv)
	_copy_pipeline = _rd.compute_pipeline_create(_copy_shader)
	var state := RDSamplerState.new()
	state.min_filter = RenderingDevice.SAMPLER_FILTER_LINEAR
	state.mag_filter = RenderingDevice.SAMPLER_FILTER_LINEAR
	state.repeat_u = RenderingDevice.SAMPLER_REPEAT_MODE_CLAMP_TO_EDGE
	state.repeat_v = RenderingDevice.SAMPLER_REPEAT_MODE_CLAMP_TO_EDGE
	_sampler = _rd.sampler_create(state)


func _render_callback(callback_type: int, render_data: RenderData) -> void:
	if _rd == null or not _pipeline.is_valid() or not _copy_pipeline.is_valid():
		return
	if callback_type != EFFECT_CALLBACK_TYPE_POST_TRANSPARENT:
		return
	var buffers := render_data.get_render_scene_buffers() as RenderSceneBuffersRD
	if buffers == null:
		return
	var size := buffers.get_internal_size()
	if size.x == 0 or size.y == 0:
		return
	if _copy_size != size or not _copy.is_valid():
		if _copy.is_valid():
			_rd.free_rid(_copy)
		var format := RDTextureFormat.new()
		format.format = RenderingDevice.DATA_FORMAT_R16G16B16A16_SFLOAT
		format.width = size.x
		format.height = size.y
		format.usage_bits = (
			RenderingDevice.TEXTURE_USAGE_SAMPLING_BIT | RenderingDevice.TEXTURE_USAGE_STORAGE_BIT
		)
		_copy = _rd.texture_create(format, RDTextureView.new())
		_copy_size = size
	var push := PackedFloat32Array([size.x, size.y, intensity, float(samples)]).to_byte_array()
	var groups := Vector2i(ceili(size.x / 8.0), ceili(size.y / 8.0))
	for view in buffers.get_view_count():
		var color := buffers.get_color_layer(view)
		var velocity := buffers.get_velocity_layer(view)
		var copy := _copy
		if not color.is_valid() or not velocity.is_valid():
			continue
		var read_color := RDUniform.new()
		read_color.uniform_type = RenderingDevice.UNIFORM_TYPE_IMAGE
		read_color.binding = 0
		read_color.add_id(color)
		var write_copy := RDUniform.new()
		write_copy.uniform_type = RenderingDevice.UNIFORM_TYPE_IMAGE
		write_copy.binding = 1
		write_copy.add_id(copy)
		var copy_set := UniformSetCacheRD.get_cache(_copy_shader, 0, [read_color, write_copy])
		var source_uniform := RDUniform.new()
		source_uniform.uniform_type = RenderingDevice.UNIFORM_TYPE_SAMPLER_WITH_TEXTURE
		source_uniform.binding = 0
		source_uniform.add_id(_sampler)
		source_uniform.add_id(copy)
		var velocity_uniform := RDUniform.new()
		velocity_uniform.uniform_type = RenderingDevice.UNIFORM_TYPE_SAMPLER_WITH_TEXTURE
		velocity_uniform.binding = 1
		velocity_uniform.add_id(_sampler)
		velocity_uniform.add_id(velocity)
		var output_uniform := RDUniform.new()
		output_uniform.uniform_type = RenderingDevice.UNIFORM_TYPE_IMAGE
		output_uniform.binding = 2
		output_uniform.add_id(color)
		var uniform_set := UniformSetCacheRD.get_cache(
			_shader, 0, [source_uniform, velocity_uniform, output_uniform]
		)
		var list := _rd.compute_list_begin()
		_rd.compute_list_bind_compute_pipeline(list, _copy_pipeline)
		_rd.compute_list_bind_uniform_set(list, copy_set, 0)
		_rd.compute_list_dispatch(list, groups.x, groups.y, 1)
		_rd.compute_list_add_barrier(list)
		_rd.compute_list_bind_compute_pipeline(list, _pipeline)
		_rd.compute_list_bind_uniform_set(list, uniform_set, 0)
		_rd.compute_list_set_push_constant(list, push, push.size())
		_rd.compute_list_dispatch(list, groups.x, groups.y, 1)
		_rd.compute_list_end()
