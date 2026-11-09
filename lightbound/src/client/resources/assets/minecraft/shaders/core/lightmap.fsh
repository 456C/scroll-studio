#version 330
#extension GL_ARB_separate_shader_objects : require

// Minecraft 26.3 lightmap. The negative ambient marker carries client visual
// settings without changing the vanilla uniform-buffer layout.
layout(std140) uniform LightmapInfo {
    float SkyFactor;
    float BlockFactor;
    float NightVisionFactor;
    float DarknessScale;
    float BossOverlayWorldDarkeningFactor;
    float BrightnessFactor;
    vec3 BlockLightTint;
    vec3 SkyLightColor;
    vec3 AmbientColor;
    vec3 NightVisionColor;
} lightmapInfo;

layout(location = 0) in vec2 texCoord;
layout(location = 0) out vec4 fragColor;

float light_curve(float level) {
    float value = clamp(level, 0.0, 1.0);
    return value / (4.0 - 3.0 * value);
}

vec3 adjust_brightness(vec3 color) {
    float peak = max(color.r, max(color.g, color.b));
    if (peak <= 0.0) {
        return vec3(0.0);
    }
    float raisedPeak = 1.0 - pow(1.0 - peak, 4.0);
    return mix(color, color * (raisedPeak / peak), clamp(lightmapInfo.BrightnessFactor, 0.0, 1.0));
}

void main() {
    float blockLevel = clamp(floor(texCoord.x * 16.0) / 15.0, 0.0, 1.0);
    float skyLevel = clamp(floor(texCoord.y * 16.0) / 15.0, 0.0, 1.0);

    bool hardcore = lightmapInfo.AmbientColor.r < 0.0;
    // Removing the ambient and moonlight floors reproduces the original Hardcore
    // Darkness mode-0 behavior. This is an independent implementation for 26.3.
    float sky = hardcore ? light_curve(skyLevel * lightmapInfo.SkyFactor)
                         : light_curve(skyLevel) * lightmapInfo.SkyFactor;
    float block = light_curve(blockLevel) * lightmapInfo.BlockFactor;
    vec3 warmLight = mix(vec3(1.0, 0.58, 0.28), vec3(1.0, 0.92, 0.76), blockLevel * blockLevel);
    vec3 vanillaTint = mix(lightmapInfo.BlockLightTint, vec3(1.0), 0.9 * pow(2.0 * blockLevel - 1.0, 2.0));
    vec3 ambient = hardcore ? vec3(0.0) : lightmapInfo.AmbientColor;
    vec3 color = max(ambient, lightmapInfo.NightVisionColor * lightmapInfo.NightVisionFactor);
    color += lightmapInfo.SkyLightColor * sky + (hardcore ? warmLight : vanillaTint) * block;
    if (hardcore && lightmapInfo.AmbientColor.g > 0.0) {
        float effectiveLevel = max(blockLevel * 15.0, skyLevel * lightmapInfo.SkyFactor * 15.0);
        float lit = step(lightmapInfo.AmbientColor.g - 0.0001, effectiveLevel);
        vec3 comfortable = vec3(1.0, 0.96, 0.88) * clamp(lightmapInfo.AmbientColor.b, 0.35, 1.0);
        color = max(comfortable * lit, lightmapInfo.NightVisionColor * lightmapInfo.NightVisionFactor);
    }
    color *= mix(vec3(1.0), vec3(0.7, 0.6, 0.6), lightmapInfo.BossOverlayWorldDarkeningFactor);
    color = max(vec3(0.0), color - vec3(lightmapInfo.DarknessScale));
    // Keep the hardcore curve independent of gamma: dim edges must not wash out.
    fragColor = vec4(hardcore ? clamp(color, 0.0, 1.0) : adjust_brightness(clamp(color, 0.0, 1.0)), 1.0);
}
