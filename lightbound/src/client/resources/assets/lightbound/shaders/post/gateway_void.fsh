#version 330
#extension GL_ARB_separate_shader_objects : require

#include <minecraft:globals.glsl>

uniform sampler2D InSampler;
uniform sampler2D PortalSampler;
uniform sampler2D EndSkySampler;

layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 fragColor;

vec3 rainbow(float hue) {
    vec3 wave = abs(fract(hue + vec3(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0 - 3.0);
    return mix(vec3(0.16), vec3(1.0), clamp(wave - 1.0, 0.0, 1.0));
}

void main() {
    vec4 scene = texture(InSampler, texCoord);
    // Only the essentially black unknown changes; readable terrain and the gold rim stay intact.
    float peak = max(scene.r, max(scene.g, scene.b));
    if (peak > 0.008) {
        fragColor = scene;
        return;
    }

    // One continuous field across sky and dark terrain, with no block geometry or tile grid.
    vec2 uv = (texCoord - 0.5) * vec2(ScreenSize.x / max(ScreenSize.y, 1.0), 1.0);
    vec2 drift = vec2(sin(GameTime * 6.2831853), cos(GameTime * 6.2831853)) * 0.08;
    float haze = texture(EndSkySampler, fract(uv * 0.65 + drift)).r;
    float nearStars = texture(PortalSampler, fract(uv * 1.65 + drift)).r;
    vec2 turned = mat2(0.8, -0.6, 0.6, 0.8) * uv;
    float farStars = texture(PortalSampler, fract(turned * 3.15 - drift * 0.6 + vec2(0.27, 0.49))).r;
    // GameTime wraps every 20 minutes; four complete hue cycles means a five-minute rainbow drift.
    float hue = uv.x * 0.31 + uv.y * 0.23 + haze * 0.12 + GameTime * 4.0;
    vec3 field = rainbow(hue) * (0.018 + haze * 0.055 + nearStars * 0.38)
               + rainbow(hue + 0.18) * farStars * 0.20;
    float mask = 1.0 - smoothstep(0.002, 0.008, peak);
    fragColor = vec4(mix(scene.rgb, field, mask), scene.a);
}
