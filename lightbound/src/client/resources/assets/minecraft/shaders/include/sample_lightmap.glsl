#ifndef MINECRAFT_SAMPLE_LIGHTMAP_GLSL
#define MINECRAFT_SAMPLE_LIGHTMAP_GLSL
#include <minecraft:globals.glsl>
vec4 sample_lightmap(sampler2D lightMap, ivec2 uv) {
    if (uv.y == 254 || uv.y == 255) {
        float age = mod(GameTime * 24000.0 - float(uv.x) + 240.0, 240.0);
        float amount = clamp(age / 6.0, 0.0, 1.0);
        if (uv.y == 254) amount = 1.0 - amount;
        return vec4(texture(lightMap, vec2(15.5, 0.5) / 16.0).rgb * amount, 1.0);
    }
    vec2 coordinate = (vec2(uv) + vec2(8.0)) / 256.0;
    return texture(lightMap, clamp(coordinate, vec2(0.03125), vec2(0.96875)));
}
#endif
