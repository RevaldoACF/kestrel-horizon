#version 450

layout(push_constant) uniform PushConstants {
    vec2 screen_size;
    float is_srgb;
    float _pad;
} pc;

layout(location = 0) in vec2 in_pos;
layout(location = 1) in vec2 in_uv;
layout(location = 2) in vec2 in_rect_size;
layout(location = 3) in vec4 in_color;
layout(location = 4) in vec4 in_border_color;
layout(location = 5) in vec4 in_params;

layout(location = 0) out vec2 out_uv;
layout(location = 1) out vec2 out_rect_size;
layout(location = 2) out vec4 out_color;
layout(location = 3) out vec4 out_border_color;
layout(location = 4) out vec4 out_params;

void main() {
    vec2 ndc = (in_pos / pc.screen_size) * 2.0 - 1.0;
    gl_Position = vec4(ndc, 0.0, 1.0);
    out_uv = in_uv;
    out_rect_size = in_rect_size;
    out_color = in_color;
    out_border_color = in_border_color;
    out_params = in_params;
}
