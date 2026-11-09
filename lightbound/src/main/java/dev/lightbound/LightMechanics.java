package dev.lightbound;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.ConcurrentHashMap;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLevelEvents;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;

/** Optional mechanics share the same authoritative rules and client prediction math. */
public final class LightMechanics {
    private static final Map<Level, AfterglowTrail> TRAILS = new ConcurrentHashMap<>();
    private static Level clientLevel;
    private static int localPredictionPlayerId = -1;

    private LightMechanics() {}

    public static void initialize() {
        AfterglowSync.initialize();
        ServerTickEvents.START_LEVEL_TICK.register(level -> {
            recordSources(level);
            for (ServerPlayer player : List.copyOf(level.players())) {
                tryVoidLoop(player);
                applyDarkSwimming(player, player.getLastClientInput().jump());
            }
        });
        ServerLevelEvents.UNLOAD.register((server, level) -> clearLevel(level));
        ServerLifecycleEvents.SERVER_STOPPED.register(server -> TRAILS.keySet().removeIf(level -> !level.isClientSide()));
    }

    /** Call from the client tick before capturing native dynamic lights. */
    public static void tickClient(Level level) {
        tickClient(level, localPredictionPlayerId);
    }

    public static void tickClient(Level level, int localPlayerId) {
        AfterglowSync.clientTick();
        if (level != clientLevel) {
            if (clientLevel != null) clearLevel(clientLevel);
            clientLevel = level;
        }
        localPredictionPlayerId = localPlayerId;
        if (level != null) recordSources(level);
    }

    public static void clearClient() {
        if (clientLevel != null) clearLevel(clientLevel);
        clientLevel = null;
        localPredictionPlayerId = -1;
        AfterglowSync.clearClient();
    }

    public static void clearLevel(Level level) { TRAILS.remove(level); }

    /** One bounded history capture per world tick; current held sources stay live. */
    public static void recordSources(Level level) {
        int duration = LightboundSettings.afterglowTicks(level);
        if (duration <= 0 || !LightboundSettings.isActive(level)) {
            TRAILS.remove(level);
            return;
        }
        AfterglowTrail trail = TRAILS.computeIfAbsent(level, ignored -> new AfterglowTrail());
        long tick = level.getGameTime();
        trail.advance(tick, duration);
        for (Player player : level.players()) {
            if (player.isSpectator()) continue;
            // Remote movement is interpolated on clients; only the local player's
            // fresh input is predicted. Other players' history comes from the server.
            if (level.isClientSide() && player.getId() != localPredictionPlayerId) continue;
            int luminance = LightRules.heldLuminance(player);
            if (luminance > 0) trail.record(player.getId(), player.getX(), player.getEyeY(), player.getZ(), luminance, tick);
        }
    }

    private static AfterglowTrail activeTrail(Level level) {
        int duration = LightboundSettings.afterglowTicks(level);
        if (duration <= 0 || LightRules.heartbeatDark(level)) return null;
        AfterglowTrail trail = TRAILS.get(level);
        if (trail != null) trail.advance(level.getGameTime(), duration);
        return trail;
    }

    public static List<LightRules.LightSource> afterglowSources(Level level) {
        AfterglowTrail trail = activeTrail(level);
        if (level.isClientSide()) {
            if (LightboundSettings.afterglowTicks(level) <= 0 || LightRules.heartbeatDark(level)) return List.of();
            return AfterglowSync.clientSources(level, trail == null ? List.of() : trail.sources());
        }
        if (trail == null) return List.of();
        List<LightRules.LightSource> sources = new ArrayList<>();
        for (AfterglowTrail.Sample source : trail.sources()) {
            sources.add(new LightRules.LightSource(source.id(), source.x(), source.y(), source.z(), source.luminance()));
        }
        return List.copyOf(sources);
    }

    public static double afterglowLight(Level level, BlockPos pos) {
        AfterglowTrail trail = activeTrail(level);
        if (level.isClientSide()) {
            if (LightboundSettings.afterglowTicks(level) <= 0 || LightRules.heartbeatDark(level)) return 0;
            return AfterglowSync.clientLight(level, pos, trail == null ? List.of() : trail.sources());
        }
        return trail == null ? 0 : trail.sample(pos.getX() + 0.5, pos.getY() + 0.5, pos.getZ() + 0.5);
    }

    static List<AfterglowTrail.Sample> historySamples(Level level) {
        AfterglowTrail trail = TRAILS.get(level);
        if (trail == null) return List.of();
        trail.advance(level.getGameTime(), LightboundSettings.afterglowTicks(level));
        return trail.sources();
    }

    public static boolean darkSwimmingActive(Player player) {
        return LightboundSettings.darkSwimming(player.level()) && LightboundSettings.isActive(player.level())
                && player.isAlive() && LightRules.appliesTo(player) && !player.isPassenger()
                && !LightMath.isSolid(LightRules.getEffectiveLight(player.level(), player.blockPosition()),
                        LightboundSettings.threshold(player.level()));
    }

    /** Vanilla input already sends jump to the server; no custom movement packet is needed. */
    public static boolean applyDarkSwimming(Player player, boolean jumping) {
        if (!jumping || !darkSwimmingActive(player)) return false;
        Vec3 motion = player.getDeltaMovement();
        double rise = Math.min(0.22, Math.max(-0.08, motion.y) + 0.12);
        player.setDeltaMovement(motion.x * 0.98, rise, motion.z * 0.98);
        player.resetFallDistance();
        return true;
    }

    /** Teleport only the player, before vanilla's deeper out-of-world damage boundary. */
    public static boolean tryVoidLoop(ServerPlayer player) {
        ServerLevel level = player.level();
        if (!LightboundSettings.voidLoop(level) || !player.isAlive() || player.isSpectator()
                || player.getY() >= level.getMinY() - 32.0) return false;
        player.stopRiding();
        boolean moved = player.teleportTo(level, player.getX(), level.getMaxY() + 16.0, player.getZ(),
                Set.of(), player.getYRot(), player.getXRot(), false);
        if (moved) {
            player.setDeltaMovement(Vec3.ZERO);
            player.resetFallDistance();
        }
        return moved;
    }
}
