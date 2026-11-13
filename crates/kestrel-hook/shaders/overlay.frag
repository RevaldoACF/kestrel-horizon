#version 450

layout(push_constant) uniform PushConstants {
    vec2 screen_size;
    float is_srgb;
    float _pad;
} pc;

layout(set = 0, binding = 0) uniform sampler2D font_atlas;

layout(location = 0) in vec2 in_uv;
layout(location = 1) in vec2 in_rect_size;
layout(location = 2) in vec4 in_color;
layout(location = 3) in vec4 in_border_color;
layout(location = 4) in vec4 in_params;

layout(location = 0) out vec4 out_frag;

vec3 srgb_to_linear(vec3 c) {
    bvec3 cutoff = lessThanEqual(c, vec3(0.04045));
    vec3 higher = pow((c + vec3(0.055)) / vec3(1.055), vec3(2.4));
    vec3 lower = c / vec3(12.92);
    return mix(higher, lower, cutoff);
}

float sd_rounded_box(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + vec2(r);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

vec4 to_target_color(vec4 c) {
    if (pc.is_srgb > 0.5) {
        return vec4(srgb_to_linear(c.rgb), c.a);
    }
    return c;
}

void main() {
    int mode = int(in_params.z + 0.5);
    vec4 col = to_target_color(in_color);

    if (mode == 1) {
        // Textured glyph from R8/RGBA8 atlas
        float cov = texture(font_atlas, in_uv).a;
        float a = col.a * cov;
        if (a <= 0.002) discard;
        out_frag = vec4(col.rgb * a, a);
        return;
    }

    vec2 half_size = max(in_rect_size * 0.5, vec2(0.5));
    vec2 p = in_uv - half_size;
    float r = clamp(in_params.x, 0.0, min(half_size.x, half_size.y));
    float d = sd_rounded_box(p, half_size, r);

    if (mode == 2) {
        // Elevation shadow
        float spread = max(in_params.w, 1.0);
        float a = (1.0 - smoothstep(-spread * 0.25, spread, d)) * col.a;
        if (a <= 0.002) discard;
        out_frag = vec4(col.rgb * a, a);
        return;
    }

    if (mode == 3) {
        // SDF Ring / Circle
        float radius = max(in_params.x, 1.0);
        float thickness = max(in_params.y, 1.0);
        float dist_center = length(p);
        float ring_dist = abs(dist_center - radius) - thickness * 0.5;
        float a = (1.0 - smoothstep(-0.7, 0.7, ring_dist)) * col.a;
        if (a <= 0.002) discard;
        out_frag = vec4(col.rgb * a, a);
        return;
    }

    // Mode 0: Rounded rectangle with optional crisp inner border
    float outer_a = 1.0 - smoothstep(-0.65, 0.65, d);
    if (outer_a <= 0.002) discard;

    float bw = in_params.y;
    vec4 final_col = col;
    if (bw > 0.01) {
        vec4 bcol = to_target_color(in_border_color);
        float inner_mask = smoothstep(-0.65, 0.65, d + bw);
        // Premultiplied blend between fill and border
        vec3 fill_pm = col.rgb * col.a;
        vec3 border_pm = bcol.rgb * bcol.a;
        vec3 mix_pm = mix(fill_pm, border_pm, inner_mask);
        float mix_a = mix(col.a, bcol.a, inner_mask) * outer_a;
        out_frag = vec4(mix_pm * outer_a, mix_a);
        return;
    }

    float a = col.a * outer_a;
    out_frag = vec4(col.rgb * a, a);
}
