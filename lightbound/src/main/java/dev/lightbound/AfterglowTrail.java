package dev.lightbound;

import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Iterator;
import java.util.List;
import java.util.Map;

/** Bounded, deterministic light history; no blocks, entities, or chunk tickets are retained. */
public final class AfterglowTrail {
    public static final int MAX_SOURCES_PER_PLAYER = 40;
    public static final int MAX_SOURCES_TOTAL = 4096;
    private final Map<Integer, ArrayDeque<Sample>> players = new HashMap<>();
    private long nextId = -1;
    private long lastTick = Long.MIN_VALUE;
    private int duration;
    private int sourceCount;
    private List<Sample> snapshot;
    private Map<Long, List<Sample>> index;

    public void advance(long gameTime, int durationTicks) {
        int nextDuration = Math.clamp(durationTicks, 0, 200);
        if (gameTime < lastTick || nextDuration == 0) clear();
        if (gameTime == lastTick && nextDuration == duration) return;
        lastTick = gameTime;
        duration = nextDuration;
        for (Iterator<ArrayDeque<Sample>> owners = players.values().iterator(); owners.hasNext();) {
            ArrayDeque<Sample> samples = owners.next();
            int previousSize = samples.size();
            boolean removed = samples.removeIf(sample -> !LightMath.afterglowActive(sample.capturedTick, gameTime, duration));
            sourceCount -= previousSize - samples.size();
            if (removed) invalidate();
            if (samples.isEmpty()) owners.remove();
        }
    }

    public void record(int owner, double x, double y, double z, int luminance, long gameTime) {
        if (duration == 0 || luminance <= 0 || !Double.isFinite(x + y + z)) return;
        ArrayDeque<Sample> samples = players.computeIfAbsent(owner, key -> new ArrayDeque<>());
        Sample previous = samples.peekLast();
        int bucketTicks = Math.max(1, (duration + MAX_SOURCES_PER_PLAYER - 1) / MAX_SOURCES_PER_PLAYER);
        boolean samePoint = previous != null && previous.luminance == luminance
                && previous.x == x && previous.y == y && previous.z == z;
        boolean sameBucket = previous != null && previous.luminance == luminance
                && Math.floorDiv(previous.capturedTick, bucketTicks) == Math.floorDiv(gameTime, bucketTicks);
        long id;
        if (samePoint || sameBucket) {
            id = samples.removeLast().id;
        } else {
            id = nextId--;
            sourceCount++;
        }
        samples.addLast(new Sample(id, x, y, z, Math.clamp(luminance, 0, 15), gameTime));
        while (samples.size() > MAX_SOURCES_PER_PLAYER) {
            samples.removeFirst();
            sourceCount--;
        }
        while (sourceCount > MAX_SOURCES_TOTAL) {
            ArrayDeque<Sample> oldest = null;
            for (var candidate : players.values()) {
                if (oldest == null || candidate.getFirst().capturedTick < oldest.getFirst().capturedTick) oldest = candidate;
            }
            if (oldest == null) break;
            oldest.removeFirst();
            sourceCount--;
            if (oldest.isEmpty()) players.values().remove(oldest);
        }
        invalidate();
    }

    public List<Sample> sources() {
        if (snapshot == null) {
            List<Sample> all = new ArrayList<>();
            for (var samples : players.values()) all.addAll(samples);
            snapshot = List.copyOf(all);
        }
        return snapshot;
    }

    /** Only the handful of nearby historical emitters are visited by player collision. */
    public double sample(double x, double y, double z) {
        if (players.isEmpty()) return 0;
        if (index == null) {
            index = new HashMap<>();
            for (Sample source : sources()) {
                for (int sx = section(source.x - LightMath.DYNAMIC_RADIUS); sx <= section(source.x + LightMath.DYNAMIC_RADIUS); sx++) {
                    for (int sy = section(source.y - LightMath.DYNAMIC_RADIUS); sy <= section(source.y + LightMath.DYNAMIC_RADIUS); sy++) {
                        for (int sz = section(source.z - LightMath.DYNAMIC_RADIUS); sz <= section(source.z + LightMath.DYNAMIC_RADIUS); sz++) {
                            index.computeIfAbsent(key(sx, sy, sz), ignored -> new ArrayList<>()).add(source);
                        }
                    }
                }
            }
        }
        List<Sample> nearby = index.get(key(section(x), section(y), section(z)));
        if (nearby == null) return 0;
        double light = 0;
        for (Sample source : nearby) {
            double dx = source.x - x;
            double dy = source.y - y;
            double dz = source.z - z;
            double distanceSquared = dx * dx + dy * dy + dz * dz;
            if (distanceSquared < LightMath.DYNAMIC_RADIUS * LightMath.DYNAMIC_RADIUS) {
                light = Math.max(light, LightMath.dynamicLight(source.luminance, Math.sqrt(distanceSquared)));
            }
        }
        return light;
    }

    public void clear() {
        players.clear();
        lastTick = Long.MIN_VALUE;
        duration = 0;
        sourceCount = 0;
        invalidate();
    }

    private void invalidate() {
        snapshot = null;
        index = null;
    }

    private static int section(double position) { return (int) Math.floor(position / 16.0); }
    private static long key(int x, int y, int z) {
        return ((long)x & 0x3FFFFF) << 42 | ((long)z & 0x3FFFFF) << 20 | ((long)y & 0xFFFFF);
    }

    public record Sample(long id, double x, double y, double z, int luminance, long capturedTick) {}
}
