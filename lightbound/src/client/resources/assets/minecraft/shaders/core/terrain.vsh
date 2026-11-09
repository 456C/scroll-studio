#version 330
#extension GL_ARB_separate_shader_objects : require

#include <minecraft:fog.glsl>
#include <minecraft:globals.glsl>
#include <minecraft:projection.glsl>
#include <minecraft:sample_lightmap.glsl>
#include <minecraft:terrainglobals.glsl>
#ifndef MULTIDRAW_TERRAIN
#include <minecraft:chunksection.glsl>
#endif

layout(location = 0) in vec3 Position;
layout(location = 1) in vec4 Color;
layout(location = 2) in vec2 UV0;
layout(location = 3) in ivec2 UV2;
#ifdef MULTIDRAW_TERRAIN
layout(location = 4) in ivec3 ChunkPosition;
layout(location = 5) in float ChunkVisibility;
#endif

layout(location = 0) out float sphericalVertexDistance;
layout(location = 1) out float cylindricalVertexDistance;
layout(location = 2) out vec4 vertexColor;
layout(location = 3) out vec2 texCoord0;
layout(location = 4) out float chunkVisibility;

#ifndef OIT_ALPHA_ONLY
uniform sampler2D Sampler2;

vec4 terrainLight(ivec2 uv, float distanceToCamera) {
    if (uv.y == 252 || uv.y == 253) {
        vec4 fading = sample_lightmap(Sampler2, uv + ivec2(0, 2));
        return mix(fading, sample_lightmap(Sampler2, ivec2(0)), smoothstep(12.0, 24.0, distanceToCamera));
    }
    if (uv.x <= 240 || uv.x > 254 || uv.y >= 254)
        return sample_lightmap(Sampler2, uv);
    int sky = uv.y & 240;
    int nearBlock = uv.x < 248 ? (uv.x - 240) * 16 : (uv.x - 248) * 16 + 8;
    vec4 nearby = sample_lightmap(Sampler2, ivec2(nearBlock, sky));
    vec4 distant = sample_lightmap(Sampler2, ivec2((uv.y & 15) * 16, sky));
    return mix(nearby, distant, smoothstep(12.0, 24.0, distanceToCamera));
}
#endif

void main() {
    vec3 relative = Position + (ChunkPosition - CameraBlockPos) + CameraOffset;
    gl_Position = ProjMat * ModelViewMat * vec4(relative, 1.0);
    sphericalVertexDistance = fog_spherical_distance(relative);
    cylindricalVertexDistance = fog_cylindrical_distance(relative);
    vertexColor = Color;
#ifndef OIT_ALPHA_ONLY
    vertexColor *= terrainLight(UV2, sphericalVertexDistance);
#endif
    texCoord0 = UV0;
    chunkVisibility = mix(1.0, ChunkVisibility, clamp((sphericalVertexDistance - 16.0) / 16.0, 0.0, 1.0));
}
