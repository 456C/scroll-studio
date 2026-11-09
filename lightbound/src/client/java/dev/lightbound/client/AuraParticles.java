package dev.lightbound.client;

import dev.lightbound.VoidEdgeScanner.Edge;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import net.minecraft.client.multiplayer.ClientLevel;

/** Small client-only sparks attached to confirmed edges, independent of world lighting. */
public final class AuraParticles {
    public static final int MAX_PARTICLES = 64;
    public static final int MAX_SPAWNS_PER_TICK = 3;
    public static final int MAX_LIFETIME_TICKS = 19;

    private static ClientLevel lastLevel;
    private static long lastTick = Long.MIN_VALUE;
    private static long sequence = 1;
    private static double spawnCredit;
    private static List<Edge> lastEdges = List.of();
    private static Set<Edge> activeEdges = Set.of();
    private static final List<Mote> motes = new ArrayList<>(MAX_PARTICLES);
    private static List<Spark> snapshot = List.of();

    private AuraParticles() {}

    public static List<Spark> update(ClientLevel level, List<Edge> edges, long tick,
            boolean enabled, double density) {
        if (!enabled || density <= 0 || edges.isEmpty()) {
            reset();
            return List.of();
        }
        if (lastLevel != level || tick < lastTick) reset();
        lastLevel = level;
        boolean edgesChanged = edges != lastEdges;
        if (edgesChanged) {
            lastEdges = edges;
            activeEdges = new HashSet<>(edges);
            // When a light moves, motes must not mark a boundary that no longer exists.
            motes.removeIf(mote -> !activeEdges.contains(mote.edge));
        }
        if (tick != lastTick) {
            int elapsed = lastTick == Long.MIN_VALUE ? 1 : (int)Math.min(20, tick - lastTick);
            lastTick = tick;
            motes.removeIf(mote -> tick - mote.born >= mote.lifetime);
            spawnCredit = Math.min(MAX_SPAWNS_PER_TICK,
                    spawnCredit + Math.clamp(density, 0.0, 1.0) * 3.0 * elapsed);
            int spawns = Math.min(MAX_SPAWNS_PER_TICK, (int)spawnCredit);
            spawnCredit -= spawns;
            for (int i = 0; i < spawns && motes.size() < MAX_PARTICLES; i++) {
                long random = mix(sequence++);
                Edge edge = edges.get((int)Math.floorMod(random, edges.size()));
                double t = ((random >>> 16) & 65535) / 65535.0;
                double drift = (((random >>> 32) & 255) / 255.0 - 0.5) * 0.006;
                double rise = 0.012 + ((random >>> 40) & 255) / 255.0 * 0.008;
                int lifetime = 12 + (int)((random >>> 48) & 7);
                double size = 0.025 + ((random >>> 56) & 127) / 127.0 * 0.025;
                motes.add(new Mote(edge, tick, lifetime, t, drift, rise, size));
            }
            snapshot = snapshot(tick);
        } else if (edgesChanged) {
            snapshot = snapshot(tick);
        }
        return snapshot;
    }

    private static List<Spark> snapshot(long tick) {
        List<Spark> sparks = new ArrayList<>(motes.size());
        for (Mote mote : motes) {
            double age = Math.max(0, tick - mote.born);
            float opacity = (float)(Math.min(1.0, (age + 1.0) / 3.0)
                    * Math.min(1.0, (mote.lifetime - age) / 6.0));
            Edge edge = mote.edge;
            double dx = edge.x2() - edge.x1();
            double dz = edge.z2() - edge.z1();
            sparks.add(new Spark(
                    edge.x1() + dx * mote.t - dz * age * mote.drift,
                    edge.y1() + 0.035 + age * mote.rise,
                    edge.z1() + dz * mote.t + dx * age * mote.drift,
                    mote.size, opacity));
        }
        return List.copyOf(sparks);
    }

    public static int liveCount() {
        return motes.size();
    }

    public static void reset() {
        motes.clear();
        snapshot = List.of();
        activeEdges = Set.of();
        lastEdges = List.of();
        lastLevel = null;
        lastTick = Long.MIN_VALUE;
        spawnCredit = 0;
    }

    private static long mix(long value) {
        value ^= value >>> 30;
        value *= 0xbf58476d1ce4e5b9L;
        value ^= value >>> 27;
        value *= 0x94d049bb133111ebL;
        return value ^ (value >>> 31);
    }

    private record Mote(Edge edge, long born, int lifetime, double t, double drift, double rise, double size) {}
    public record Spark(double x, double y, double z, double size, float opacity) {}
}
