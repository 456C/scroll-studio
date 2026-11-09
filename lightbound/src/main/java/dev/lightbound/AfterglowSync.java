package dev.lightbound;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents;
import net.fabricmc.fabric.api.networking.v1.PayloadTypeRegistry;
import net.fabricmc.fabric.api.networking.v1.ServerPlayConnectionEvents;
import net.fabricmc.fabric.api.networking.v1.ServerPlayNetworking;
import net.minecraft.core.BlockPos;
import net.minecraft.core.SectionPos;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.Vec3;

/** Authoritative nearby history seeds new clients and corrects prediction twice a second. */
public final class AfterglowSync {
    public static final int UPDATE_INTERVAL = 10;
    public static final double SYNC_RADIUS = 40;
    private static final Map<UUID, Subscription> SUBSCRIBERS = new HashMap<>();
    private static HistoryPayload received;
    private static long clientTicks;
    private static long viewTick = Long.MIN_VALUE;
    private static int viewDuration;
    private static List<AfterglowTrail.Sample> viewLocal;
    private static Level viewLevel;
    private static List<LightRules.LightSource> view = List.of();
    private static Map<Long, List<LightRules.LightSource>> viewIndex = Map.of();

    private AfterglowSync() {}

    public static void initialize() {
        PayloadTypeRegistry.clientboundPlay().register(HistoryPayload.TYPE, HistoryPayload.CODEC);
        ServerPlayConnectionEvents.JOIN.register((handler, sender, server) -> send(handler.player, true));
        ServerPlayConnectionEvents.DISCONNECT.register((handler, server) -> SUBSCRIBERS.remove(handler.player.getUUID()));
        ServerTickEvents.END_LEVEL_TICK.register(level -> {
            if (LightboundSettings.afterglowTicks(level) == 0 && SUBSCRIBERS.isEmpty()) return;
            for (ServerPlayer player : level.players()) send(player, false);
        });
        ServerLifecycleEvents.SERVER_STOPPED.register(server -> SUBSCRIBERS.clear());
    }

    private static void send(ServerPlayer player, boolean force) {
        ServerLevel level = player.level();
        int duration = LightboundSettings.afterglowTicks(level);
        Subscription old = SUBSCRIBERS.get(player.getUUID());
        if (duration == 0 && old == null) return;
        if (!ServerPlayNetworking.canSend(player, HistoryPayload.TYPE)) return;
        Identifier dimension = level.dimension().identifier();
        long tick = level.getGameTime();
        Vec3 position = player.position();
        boolean changedContext = old == null || !dimension.equals(old.dimension) || duration != old.duration
                || position.distanceToSqr(old.position) > 8 * 8 || tick < old.tick;
        if (!force && !changedContext && tick - old.tick < UPDATE_INTERVAL) return;
        List<AfterglowTrail.Sample> nearby = new ArrayList<>();
        if (duration > 0) {
            for (AfterglowTrail.Sample source : LightMechanics.historySamples(level)) {
                double dx = source.x() - position.x;
                double dy = source.y() - player.getEyeY();
                double dz = source.z() - position.z;
                if (dx * dx + dy * dy + dz * dz <= SYNC_RADIUS * SYNC_RADIUS) nearby.add(source);
            }
        }
        List<AfterglowTrail.Sample> sources = List.copyOf(nearby);
        // A stable abandoned path needs no repeated packets: its captured ticks expire locally.
        if (force || changedContext || !sources.equals(old.sources)) {
            ServerPlayNetworking.send(player, new HistoryPayload(dimension, tick, duration, sources));
        }
        if (duration == 0) SUBSCRIBERS.remove(player.getUUID());
        else SUBSCRIBERS.put(player.getUUID(), new Subscription(dimension, tick, duration, position, sources));
    }

    public static void accept(HistoryPayload payload) {
        received = payload;
        invalidateClientView();
    }

    public static void clientTick() {
        clientTicks++;
        invalidateClientView();
    }

    public static void clearClient() {
        received = null;
        viewLocal = null;
        viewLevel = null;
        view = List.of();
        viewIndex = Map.of();
        invalidateClientView();
    }

    private static void invalidateClientView() { viewTick = Long.MIN_VALUE; }

    public static List<LightRules.LightSource> clientSources(Level level, List<AfterglowTrail.Sample> predicted) {
        int duration = LightboundSettings.afterglowTicks(level);
        if (viewLevel == level && viewTick == clientTicks && viewDuration == duration && viewLocal == predicted) return view;
        viewLevel = level;
        viewTick = clientTicks;
        viewDuration = duration;
        viewLocal = predicted;
        List<LightRules.LightSource> merged = new ArrayList<>();
        long acknowledged = Long.MIN_VALUE;
        if (received != null && received.dimension.equals(level.dimension().identifier())) {
            HistoryPayload payload = received;
            // World time freezes with singleplayer pause and tick freeze; client callbacks do not.
            // The snapshot time also prevents negative ages before the first world-time sync.
            long now = Math.max(level.getGameTime(), payload.serverTick);
            acknowledged = payload.serverTick;
            for (AfterglowTrail.Sample source : payload.sources) {
                if (LightMath.afterglowActive(source.capturedTick(), now, Math.min(duration, payload.duration))) {
                    merged.add(lightSource(source, source.id()));
                }
            }
        }
        // Fresh local input remains responsive while the next authoritative history arrives.
        // Acknowledged or older predictions are replaced, so joins/transfers cannot lose paths.
        long oldestPrediction = level.getGameTime() - UPDATE_INTERVAL - 4;
        for (AfterglowTrail.Sample source : predicted) {
            if (source.capturedTick() > acknowledged && source.capturedTick() >= oldestPrediction
                    && LightMath.afterglowActive(source.capturedTick(), level.getGameTime(), duration)) {
                merged.add(lightSource(source, Long.MIN_VALUE / 2 + source.id()));
            }
        }
        view = List.copyOf(merged);
        Map<Long, List<LightRules.LightSource>> index = new HashMap<>();
        for (var source : view) {
            for (int x = section(source.x() - LightMath.DYNAMIC_RADIUS); x <= section(source.x() + LightMath.DYNAMIC_RADIUS); x++) {
                for (int y = section(source.y() - LightMath.DYNAMIC_RADIUS); y <= section(source.y() + LightMath.DYNAMIC_RADIUS); y++) {
                    for (int z = section(source.z() - LightMath.DYNAMIC_RADIUS); z <= section(source.z() + LightMath.DYNAMIC_RADIUS); z++) {
                        index.computeIfAbsent(SectionPos.asLong(x, y, z), ignored -> new ArrayList<>()).add(source);
                    }
                }
            }
        }
        viewIndex = index;
        return view;
    }

    public static double clientLight(Level level, BlockPos pos, List<AfterglowTrail.Sample> predicted) {
        clientSources(level, predicted);
        var nearby = viewIndex.get(SectionPos.asLong(pos.getX() >> 4, pos.getY() >> 4, pos.getZ() >> 4));
        if (nearby == null) return 0;
        double result = 0;
        for (var source : nearby) result = Math.max(result, source.sample(pos.getX() + 0.5, pos.getY() + 0.5, pos.getZ() + 0.5));
        return result;
    }

    private static LightRules.LightSource lightSource(AfterglowTrail.Sample source, long id) {
        return new LightRules.LightSource(id, source.x(), source.y(), source.z(), source.luminance());
    }

    private static int section(double coordinate) { return (int)Math.floor(coordinate / 16.0); }
    private record Subscription(Identifier dimension, long tick, int duration, Vec3 position, List<AfterglowTrail.Sample> sources) {}

    public record HistoryPayload(Identifier dimension, long serverTick, int duration,
                                 List<AfterglowTrail.Sample> sources) implements CustomPacketPayload {
        public HistoryPayload {
            duration = Math.clamp(duration, 0, 200);
            if (sources.size() > AfterglowTrail.MAX_SOURCES_TOTAL) throw new IllegalArgumentException("Too many afterglow samples");
            sources = List.copyOf(sources);
        }
        public static final Type<HistoryPayload> TYPE = new Type<>(Identifier.fromNamespaceAndPath("lightbound", "afterglow_history"));
        public static final StreamCodec<RegistryFriendlyByteBuf, HistoryPayload> CODEC = new StreamCodec<>() {
            @Override public HistoryPayload decode(RegistryFriendlyByteBuf buffer) {
                Identifier dimension = buffer.readIdentifier();
                long tick = buffer.readVarLong();
                int duration = buffer.readVarInt();
                int count = buffer.readVarInt();
                if (count < 0 || count > AfterglowTrail.MAX_SOURCES_TOTAL) throw new IllegalArgumentException("Invalid afterglow history size");
                List<AfterglowTrail.Sample> sources = new ArrayList<>(count);
                for (int i = 0; i < count; i++) {
                    long id = -buffer.readVarLong();
                    double x = buffer.readDouble(), y = buffer.readDouble(), z = buffer.readDouble();
                    int luminance = buffer.readUnsignedByte();
                    int age = buffer.readVarInt();
                    if (!Double.isFinite(x + y + z) || luminance > 15 || age < 0 || age > 200 || id >= 0) {
                        throw new IllegalArgumentException("Invalid afterglow source");
                    }
                    sources.add(new AfterglowTrail.Sample(id, x, y, z, luminance, tick - age));
                }
                return new HistoryPayload(dimension, tick, duration, sources);
            }
            @Override public void encode(RegistryFriendlyByteBuf buffer, HistoryPayload payload) {
                buffer.writeIdentifier(payload.dimension);
                buffer.writeVarLong(payload.serverTick);
                buffer.writeVarInt(payload.duration);
                buffer.writeVarInt(payload.sources.size());
                for (AfterglowTrail.Sample source : payload.sources) {
                    buffer.writeVarLong(-source.id());
                    buffer.writeDouble(source.x());
                    buffer.writeDouble(source.y());
                    buffer.writeDouble(source.z());
                    buffer.writeByte(source.luminance());
                    buffer.writeVarInt((int)Math.clamp(payload.serverTick - source.capturedTick(), 0, 200));
                }
            }
        };
        @Override public Type<? extends CustomPacketPayload> type() { return TYPE; }
    }
}
