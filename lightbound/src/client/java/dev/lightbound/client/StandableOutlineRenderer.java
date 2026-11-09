package dev.lightbound.client;

import com.mojang.blaze3d.vertex.PoseStack;
import dev.lightbound.VoidEdgeScanner;
import dev.lightbound.VoidEdgeScanner.Edge;
import dev.lightbound.LightRules;
import dev.lightbound.LightboundSettings;
import java.util.List;
import net.fabricmc.fabric.api.client.rendering.v1.RenderStateDataKey;
import net.fabricmc.fabric.api.client.rendering.v1.level.LevelExtractionContext;
import net.fabricmc.fabric.api.client.rendering.v1.level.LevelExtractionEvents;
import net.fabricmc.fabric.api.client.rendering.v1.level.LevelRenderContext;
import net.fabricmc.fabric.api.client.rendering.v1.level.LevelRenderEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.world.phys.Vec3;

/** A local perimeter, not a wireframe: lit neighbours never get an internal edge. */
public final class StandableOutlineRenderer {
    private static final int REFRESH_TICKS = 2;
    private static final RenderStateDataKey<EffectState> EFFECTS =
            RenderStateDataKey.create(() -> "lightbound:standable_perimeter");
    private static GlowEdgeRenderer.Palette cachedPalette = GlowEdgeRenderer.palette(GlowEdgeRenderer.DEFAULT_GLOW_COLOR);
    private static final EffectState EMPTY = new EffectState(List.of(), List.of(), 0, cachedPalette);

    private static ClientLevel cachedLevel;
    private static List<Edge> cachedEdges = List.of();
    private static List<Edge> visualEdges = List.of();
    private static List<Edge> visualInput;
    private static boolean visualCurved;
    private static long lastScanTick = Long.MIN_VALUE;
    private static Vec3 lastPlayerPosition;
    private static int lastHeldLight = -1;
    private static int lastThreshold = -1;
    private static int lastSkyDarken = -1;

    private StandableOutlineRenderer() {}

    public static void initialize() {
        LevelExtractionEvents.END_EXTRACTION.register(StandableOutlineRenderer::extract);
        LevelRenderEvents.COLLECT_SUBMITS.register(StandableOutlineRenderer::submit);
    }

    private static void extract(LevelExtractionContext context) {
        ClientLevel level = context.level();
        var client = Minecraft.getInstance();
        var player = client.player;
        boolean edgeGlow = ClientVisualSettings.edgeGlow;
        boolean particles = ClientVisualSettings.auraParticles && ClientVisualSettings.particleDensity > 0;
        float strength = (float)Math.clamp(ClientVisualSettings.glowStrength, 0.0, 2.0);
        if (player == null || !LightboundSettings.isActive(level) || player.isSpectator()
                || client.gui.hud.isHidden()
                || (!edgeGlow && !particles) || strength <= 0) {
            cachedLevel = null;
            cachedEdges = List.of();
            AuraParticles.reset();
            context.levelState().setData(EFFECTS, EMPTY);
            return;
        }

        long tick = context.levelState().gameTime;
        // Two brief bright beats announce the optional blackout without moving its rim.
        int period = LightboundSettings.heartbeatPeriodTicks(level);
        if (period > 0 && !LightRules.heartbeatDark(level)) {
            int litTicks = period - LightboundSettings.heartbeatDarkTicks(level);
            int warningTicks = Math.min(20, Math.max(1, litTicks / 3));
            long phase = Math.floorMod(tick, period);
            if (phase >= litTicks - warningTicks) {
                double progress = (phase - (litTicks - warningTicks)) / (double)warningTicks;
                double beat = Math.max(0, Math.sin(progress * Math.PI * 4));
                strength *= (float)(1.0 + 0.65 * beat * beat);
            }
        }
        BlockPos playerBlock = player.blockPosition();
        int heldLight = LightRules.heldLuminance(player);
        int threshold = LightboundSettings.threshold(level);
        int skyDarken = level.getSkyDarken();
        boolean worldChanged = level != cachedLevel;
        boolean invalidated = worldChanged || !player.position().equals(lastPlayerPosition)
                || heldLight != lastHeldLight || threshold != lastThreshold
                || skyDarken != lastSkyDarken;

        // At most one bounded scan per game tick, even at uncapped frame rates.
        // Still refresh while stationary, so lamps, mining and other players update it.
        if (worldChanged || (tick != lastScanTick
                && (invalidated || tick < lastScanTick || tick - lastScanTick >= REFRESH_TICKS))) {
            cachedEdges = VoidEdgeScanner.scan(level, playerBlock, player.getY(), player.getBbHeight(), threshold);
            cachedLevel = level;
            lastScanTick = tick;
            lastPlayerPosition = player.position();
            lastHeldLight = heldLight;
            lastThreshold = threshold;
            lastSkyDarken = skyDarken;
        }
        if (visualInput != cachedEdges || visualCurved != ClientVisualSettings.curvedEdges) {
            visualInput = cachedEdges;
            visualCurved = ClientVisualSettings.curvedEdges;
            visualEdges = visualCurved ? dev.lightbound.EdgeContours.round(cachedEdges) : cachedEdges;
        }
        // The same confirmed boundary drives both effects; particles never scan the world.
        List<AuraParticles.Spark> sparks = AuraParticles.update(level, visualEdges, tick,
                particles, ClientVisualSettings.particleDensity);
        int glowColor = ClientVisualSettings.glowColor & 0xFFFFFF;
        if (glowColor != cachedPalette.rgb()) cachedPalette = GlowEdgeRenderer.palette(glowColor);
        // Only immutable geometry and a settings snapshot cross to the render thread.
        context.levelState().setData(EFFECTS,
                new EffectState(edgeGlow ? visualEdges : List.of(), sparks, strength, cachedPalette));
    }

    private static void submit(LevelRenderContext context) {
        EffectState effects = context.levelState().getDataOrDefault(EFFECTS, EMPTY);
        if (effects.edges.isEmpty() && effects.sparks.isEmpty()) return;
        Vec3 camera = context.levelState().cameraRenderState.pos;
        PoseStack pose = context.poseStack();
        pose.pushPose();
        // One translucent, depth-tested batch. Hidden floors stay hidden behind walls.
        context.submitNodeCollector().submitCustomGeometry(pose, GlowEdgeRenderer.renderType(),
                (transform, vertices) -> {
                    for (Edge edge : effects.edges) GlowEdgeRenderer.emit(transform, vertices,
                            edge.x1(), edge.y1(), edge.z1(), edge.x2(), edge.y2(), edge.z2(), camera,
                            effects.strength, effects.palette);
                    for (AuraParticles.Spark spark : effects.sparks)
                        GlowEdgeRenderer.emitSpark(transform, vertices, spark, camera, effects.strength, effects.palette);
                });
        pose.popPose();
    }

    private record EffectState(List<Edge> edges, List<AuraParticles.Spark> sparks,
            float strength, GlowEdgeRenderer.Palette palette) {}
}
