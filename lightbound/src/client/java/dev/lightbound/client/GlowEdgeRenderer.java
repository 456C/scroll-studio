package dev.lightbound.client;

import com.mojang.blaze3d.vertex.PoseStack;
import com.mojang.blaze3d.vertex.VertexConsumer;
import net.minecraft.client.renderer.rendertype.RenderType;
import net.minecraft.client.renderer.rendertype.RenderTypes;
import net.minecraft.world.phys.Vec3;

/** Fixed-cost, depth-tested color aura with a pale core; no bloom render target. */
public final class GlowEdgeRenderer {
    public static final int LAYER_COUNT = 6;
    public static final int DEFAULT_GLOW_COLOR = 0xFFB52E;

    // Outer layers stay faint, so overlapping corners do not become solid blobs.
    // Minecraft's line shader expands these widths in screen space.
    private static final float[] WIDTHS = {26.0f, 18.0f, 11.0f, 7.0f, 4.5f, 2.0f};
    private static final int[] COLORS = {
            0x0DFF9A20, 0x18FFAD28, 0x2EFFBE38,
            0x50FFD460, 0xFFFFB52E, 0xFFFFF5D1
    };
    private static final int[] SPARK_COLORS = {0x22FFAC28, 0xA0FFD66A, 0xFFFFF5D1};
    private static final Palette DEFAULT_PALETTE = new Palette(DEFAULT_GLOW_COLOR);

    private GlowEdgeRenderer() {}

    public static RenderType renderType() {
        // Ordinary lines() ignores alpha and writes depth. This pipeline blends,
        // tests scene depth, and leaves depth untouched for the nested glow layers.
        return RenderTypes.linesTranslucentNoDepthWrite();
    }

    public static void emit(PoseStack.Pose transform, VertexConsumer vertices,
            double x1, double y1, double z1, double x2, double y2, double z2, Vec3 camera) {
        emit(transform, vertices, x1, y1, z1, x2, y2, z2, camera, 1.0f);
    }

    public static void emit(PoseStack.Pose transform, VertexConsumer vertices,
            double x1, double y1, double z1, double x2, double y2, double z2, Vec3 camera, float strength) {
        emit(transform, vertices, x1, y1, z1, x2, y2, z2, camera, strength, DEFAULT_PALETTE);
    }

    public static void emit(PoseStack.Pose transform, VertexConsumer vertices,
            double x1, double y1, double z1, double x2, double y2, double z2, Vec3 camera,
            float strength, Palette palette) {
        strength = Math.clamp(strength, 0.0f, 2.0f);
        if (strength == 0) return;
        double dx = x2 - x1;
        double dy = y2 - y1;
        double dz = z2 - z1;
        double lengthSquared = dx * dx + dy * dy + dz * dz;
        if (lengthSquared < 1.0e-12) return;
        double inverseLength = 1.0 / Math.sqrt(lengthSquared);
        float nx = (float) (dx * inverseLength);
        float ny = (float) (dy * inverseLength);
        float nz = (float) (dz * inverseLength);
        // Preserve precision far from world origin before converting to GPU floats.
        float ax = (float) (x1 - camera.x);
        float ay = (float) (y1 - camera.y);
        float az = (float) (z1 - camera.z);
        float bx = (float) (x2 - camera.x);
        float by = (float) (y2 - camera.y);
        float bz = (float) (z2 - camera.z);
        for (int layer = 0; layer < LAYER_COUNT; layer++) {
            int color = withOpacity(palette.edges[layer], strength);
            float width = WIDTHS[layer] * (0.55f + 0.45f * strength);
            vertices.addVertex(transform, ax, ay, az).setColor(color)
                    .setNormal(transform, nx, ny, nz).setLineWidth(width);
            vertices.addVertex(transform, bx, by, bz).setColor(color)
                    .setNormal(transform, nx, ny, nz).setLineWidth(width);
        }
    }

    /** Tiny three-axis spark, visible from any view, with no particle texture or light source. */
    public static void emitSpark(PoseStack.Pose pose, VertexConsumer vertices,
            AuraParticles.Spark spark, Vec3 camera, float strength) {
        emitSpark(pose, vertices, spark, camera, strength, DEFAULT_PALETTE);
    }

    public static void emitSpark(PoseStack.Pose pose, VertexConsumer vertices,
            AuraParticles.Spark spark, Vec3 camera, float strength, Palette palette) {
        float opacity = spark.opacity() * Math.clamp(strength, 0.0f, 2.0f);
        if (opacity <= 0) return;
        double r = spark.size();
        for (int axis = 0; axis < 3; axis++) {
            double dx = axis == 0 ? r : 0;
            double dy = axis == 1 ? r * 1.6 : 0;
            double dz = axis == 2 ? r : 0;
            float ax = (float)(spark.x() - dx - camera.x);
            float ay = (float)(spark.y() - dy - camera.y);
            float az = (float)(spark.z() - dz - camera.z);
            float bx = (float)(spark.x() + dx - camera.x);
            float by = (float)(spark.y() + dy - camera.y);
            float bz = (float)(spark.z() + dz - camera.z);
            sparkLine(pose, vertices, ax, ay, az, bx, by, bz, axis, 7.0f, withOpacity(palette.sparks[0], opacity));
            sparkLine(pose, vertices, ax, ay, az, bx, by, bz, axis, 3.0f, withOpacity(palette.sparks[1], opacity));
            sparkLine(pose, vertices, ax, ay, az, bx, by, bz, axis, 1.25f, withOpacity(palette.sparks[2], opacity));
        }
    }

    private static void sparkLine(PoseStack.Pose pose, VertexConsumer vertices,
            float ax, float ay, float az, float bx, float by, float bz, int axis, float width, int color) {
        float nx = axis == 0 ? 1 : 0;
        float ny = axis == 1 ? 1 : 0;
        float nz = axis == 2 ? 1 : 0;
        vertices.addVertex(pose, ax, ay, az).setColor(color).setNormal(pose, nx, ny, nz).setLineWidth(width);
        vertices.addVertex(pose, bx, by, bz).setColor(color).setNormal(pose, nx, ny, nz).setLineWidth(width);
    }

    private static int withOpacity(int color, float factor) {
        int alpha = Math.clamp(Math.round((color >>> 24) * factor), 0, 255);
        return (alpha << 24) | (color & 0xFFFFFF);
    }

    /** Build only when the chosen color changes, then share this immutable palette with rendering. */
    public static Palette palette(int rgb) {
        rgb &= 0xFFFFFF;
        return rgb == DEFAULT_GLOW_COLOR ? DEFAULT_PALETTE : new Palette(rgb);
    }

    public static final class Palette {
        private final int rgb;
        private final int[] edges = new int[LAYER_COUNT];
        private final int[] sparks = new int[SPARK_COLORS.length];

        private Palette(int rgb) {
            this.rgb = rgb;
            for (int i = 0; i < edges.length; i++) edges[i] = tint(COLORS[i], rgb);
            for (int i = 0; i < sparks.length; i++) sparks[i] = tint(SPARK_COLORS[i], rgb);
        }

        public int rgb() {
            return rgb;
        }
    }

    private static int tint(int template, int rgb) {
        // Keep the established gold appearance byte-for-byte at the default.
        if (rgb == DEFAULT_GLOW_COLOR) return template;
        float red = ((rgb >>> 16) & 255) / 255.0f;
        float green = ((rgb >>> 8) & 255) / 255.0f;
        float blue = (rgb & 255) / 255.0f;
        float high = Math.max(red, Math.max(green, blue));
        float low = Math.min(red, Math.min(green, blue));
        float delta = high - low;
        float hue = delta == 0 ? 0 : high == red ? (green - blue) / delta
                : high == green ? (blue - red) / delta + 2 : (red - green) / delta + 4;
        hue /= 6.0f;
        float saturation = high == 0 ? 0 : delta / high;

        // Relative hue/saturation preserve the hot rim and softer outside bands.
        // The original pale core becomes a white blend of any selected color.
        int templateGreen = (template >>> 8) & 255;
        int templateBlue = template & 255;
        float templateDelta = 255 - templateBlue;
        float ratio = templateDelta / 209.0f;
        hue += (templateGreen - templateBlue) / (6.0f * templateDelta) - 135.0f / 1254.0f;
        saturation = Math.clamp(saturation * ratio, 0.0f, 1.0f);
        float brightness = high + (1 - high) * Math.max(0, 1 - ratio);
        return (template & 0xFF000000) | hsv(hue, saturation, brightness);
    }

    private static int hsv(float hue, float saturation, float value) {
        float sector = (hue - (float)Math.floor(hue)) * 6.0f;
        int index = (int)sector;
        float fraction = sector - index;
        float p = value * (1 - saturation);
        float q = value * (1 - saturation * fraction);
        float t = value * (1 - saturation * (1 - fraction));
        float red, green, blue;
        switch (index) {
            case 0 -> { red = value; green = t; blue = p; }
            case 1 -> { red = q; green = value; blue = p; }
            case 2 -> { red = p; green = value; blue = t; }
            case 3 -> { red = p; green = q; blue = value; }
            case 4 -> { red = t; green = p; blue = value; }
            default -> { red = value; green = p; blue = q; }
        }
        return (Math.round(red * 255) << 16) | (Math.round(green * 255) << 8) | Math.round(blue * 255);
    }
}
