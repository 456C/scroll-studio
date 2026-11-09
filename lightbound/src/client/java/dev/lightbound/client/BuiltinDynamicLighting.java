package dev.lightbound.client;

import dev.lightbound.LightMath;
import dev.lightbound.LightRules;
import dev.lightbound.LightboundSettings;
import it.unimi.dsi.fastutil.longs.Long2ObjectMap;
import it.unimi.dsi.fastutil.longs.Long2ObjectMaps;
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongLinkedOpenHashSet;
import java.util.ArrayList;
import java.util.List;
import java.util.HashMap;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.chunk.RenderSectionRegion;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.SectionPos;
import net.minecraft.util.LightCoordsUtil;
import net.minecraft.util.Mth;
import net.minecraft.world.level.BlockAndLightGetter;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.block.state.BlockState;

/** Native vanilla-renderer lighting. It never changes a block or the world's light engine. */
public final class BuiltinDynamicLighting {
    private static final int UPDATE_INTERVAL_TICKS = 2;
    private static final int SECTION_UPDATES_PER_TICK = 32;
    private static final Direction[] FACES = Direction.values();
    private static final ThreadLocal<BlockPos.MutableBlockPos> FACE_POS = ThreadLocal.withInitial(BlockPos.MutableBlockPos::new);
    private static final Snapshot EMPTY = new Snapshot(false, false, 0, List.of(), Long2ObjectMaps.emptyMap());

    // Only immutable positions and light values cross into terrain compiler threads.
    private static volatile Snapshot current = EMPTY;
    private static final LongLinkedOpenHashSet dirtySections = new LongLinkedOpenHashSet();
    private static ClientLevel trackedLevel;
    private static long ticks;
    private static long lastSourceUpdate;

    private BuiltinDynamicLighting() {}

    public static void initialize() {
        ClientTickEvents.END_CLIENT_TICK.register(BuiltinDynamicLighting::tick);
        ClientPlayConnectionEvents.DISCONNECT.register((handler, client) -> reset());
    }

    private static void reset() {
        current = EMPTY;
        TerrainFade.clear();
        dirtySections.clear();
        trackedLevel = null;
        lastSourceUpdate = 0;
    }

    private static void tick(Minecraft client) {
        ++ticks;
        ClientLevel level = client.level;
        if (level == null || client.player == null) {
            reset();
            return;
        }
        boolean newWorld = level != trackedLevel;
        if (newWorld) {
            reset();
            trackedLevel = level;
        }

        TerrainFade.tick(client);
        Snapshot old = current;
        boolean active = LightboundSettings.isActive(level);
        boolean binary = active && ClientVisualSettings.binaryLighting;
        List<Source> sources = active ? capture(level) : List.of();
        double daylight = LightRules.daylightFactor(level);
        boolean sourceChanged = !sources.equals(old.sources);
        // Item removal, immersion, fire, joining/leaving and teleports are immediate.
        // Normal walking refreshes at 10 Hz, independent of the display frame rate.
        boolean updateSources = sourceChanged && (ticks - lastSourceUpdate >= UPDATE_INTERVAL_TICKS
                || changedEmitters(old.sources, sources));
        boolean geometryChanged = old.active != active || old.binary != binary;
        if (updateSources) {
            current = snapshot(active, binary, daylight, sources);
            lastSourceUpdate = ticks;
            queueChangedSections(old.sources, sources);
        } else if (geometryChanged || old.daylight != daylight) {
            current = new Snapshot(active, binary, daylight, old.sources, old.sections);
        }
        if (old.daylight != daylight) {
            // Drained blocks encode the exact effective light. Only their small footprint
            // needs a refresh at dawn/dusk; ordinary sky-lit terrain remains shader driven.
            for (Source source : current.sources) if (source.negative) queueSections(source);
        }

        // Binary mode uses one light sample for each whole block. A mode change needs
        // one ordinary renderer refresh; brightness and threshold remain shader-only.
        if (geometryChanged && !newWorld) {
            client.levelExtractor.allChanged();
            dirtySections.clear();
        }
        var iterator = dirtySections.iterator();
        for (int count = 0; count < SECTION_UPDATES_PER_TICK && iterator.hasNext(); ++count) {
            long section = iterator.nextLong();
            iterator.remove();
            client.levelExtractor.setSectionDirty(SectionPos.x(section), SectionPos.y(section), SectionPos.z(section));
        }
    }

    private static List<Source> capture(ClientLevel level) {
        List<Source> sources = new ArrayList<>();
        if (ClientVisualSettings.dynamicLighting) {
            for (var source : LightRules.sources(level)) {
                sources.add(new Source(source.id(), source.x(), source.y(), source.z(),
                        source.luminance(), LightMath.DYNAMIC_RADIUS, false));
            }
        }
        return List.copyOf(sources);
    }

    private static boolean changedEmitters(List<Source> previous, List<Source> next) {
        if (previous.size() != next.size()) return true;
        for (int index = 0; index < previous.size(); ++index) {
            Source before = previous.get(index);
            Source after = next.get(index);
            if (before.id != after.id || before.luminance != after.luminance || before.negative != after.negative
                    || before.distanceSquared(after.x, after.y, after.z) >= 1.0) return true;
        }
        return false;
    }

    private static Snapshot snapshot(boolean active, boolean binary, double daylight, List<Source> sources) {
        Long2ObjectMap<List<Source>> building = new Long2ObjectOpenHashMap<>();
        for (Source source : sources) {
            for (int x = section(source.x - source.radius); x <= section(source.x + source.radius); ++x) {
                for (int y = section(source.y - source.radius); y <= section(source.y + source.radius); ++y) {
                    for (int z = section(source.z - source.radius); z <= section(source.z + source.radius); ++z) {
                        building.computeIfAbsent(SectionPos.asLong(x, y, z), key -> new ArrayList<>()).add(source);
                    }
                }
            }
        }
        Long2ObjectMap<Source[]> sections = new Long2ObjectOpenHashMap<>(building.size());
        for (var entry : building.long2ObjectEntrySet()) {
            sections.put(entry.getLongKey(), entry.getValue().toArray(Source[]::new));
        }
        return new Snapshot(active, binary, daylight, sources, Long2ObjectMaps.unmodifiable(sections));
    }

    private static void queueChangedSections(List<Source> previous, List<Source> next) {
        var remaining = new HashMap<SourceKey, Source>(previous.size());
        for (Source source : previous) remaining.put(new SourceKey(source.id, source.negative), source);
        for (Source source : next) {
            Source before = remaining.remove(new SourceKey(source.id, source.negative));
            if (!source.equals(before)) {
                if (before != null) queueSections(before);
                queueSections(source);
            }
        }
        for (Source removed : remaining.values()) queueSections(removed);
    }

    private static void queueSections(Source source) {
        // Include the neighbouring AO sample halo, including sources at section edges.
        double radius = source.radius + 1.5;
        for (int x = section(source.x - radius); x <= section(source.x + radius); ++x) {
            for (int y = section(source.y - radius); y <= section(source.y + radius); ++y) {
                for (int z = section(source.z - radius); z <= section(source.z + radius); ++z) {
                    dirtySections.add(SectionPos.asLong(x, y, z));
                }
            }
        }
    }

    public static void markFadeSection(BlockPos pos) {
        dirtySections.add(SectionPos.asLong(pos.getX() >> 4, pos.getY() >> 4, pos.getZ() >> 4));
    }

    private static int section(double coordinate) {
        return Mth.floor(coordinate) >> 4;
    }

    private static boolean isWorldGeometry(BlockAndLightGetter level) {
        // Do not light inventory previews or other model-only/empty block getters.
        return level instanceof ClientLevel || level instanceof RenderSectionRegion;
    }

    public static int lightCoords(BlockAndLightGetter level, BlockPos pos, int vanilla) {
        Snapshot snapshot = current;
        if (!snapshot.active || snapshot.sources.isEmpty() || !isWorldGeometry(level)) return vanilla;
        return attenuate(addLight(vanilla, snapshot.sample(pos)), snapshot.drain(pos), snapshot.daylight, false);
    }

    /** Binary light samples the whole block. Gradient meshes only need a tag in a lichen-lit patch. */
    public static int blockLightCoords(BlockAndLightGetter level, BlockState state, BlockPos pos) {
        Snapshot snapshot = current;
        if (!snapshot.active || !isWorldGeometry(level)) return -1;
        RenderSectionRegion region = level instanceof RenderSectionRegion r ? r : null;
        LichenLighting lichen = region != null ? LichenLighting.forRegion(region) : null;
        if (!snapshot.binary) return lichen == null ? -1 : lichen.gradientLight(region, pos, snapshot.sample(pos));
        int block = Math.max(state.getLightEmission(), level.getBrightness(LightLayer.BLOCK, pos));
        Direction brightestFace = null;
        int sky = level.getBrightness(LightLayer.SKY, pos);
        BlockPos.MutableBlockPos face = FACE_POS.get();
        for (Direction direction : FACES) {
            face.setWithOffset(pos, direction);
            int faceLight = level.getBrightness(LightLayer.BLOCK, face);
            if (faceLight > block) {
                block = faceLight;
                brightestFace = direction;
            }
            sky = Math.max(sky, level.getBrightness(LightLayer.SKY, face));
        }
        double moving = snapshot.sample(pos);
        int packed = addLight(LightCoordsUtil.pack(block, sky), moving);
        int normal = LightCoordsUtil.pack(LightCoordsUtil.block(packed), LightCoordsUtil.sky(packed));
        boolean lichenOnly = false;
        if (lichen != null && block > 0 && block <= LichenLighting.EMISSION
                && moving < block) {
            BlockPos brightest = brightestFace == null ? pos : pos.relative(brightestFace);
            int other = Math.max((int)moving, lichen.otherLight(region, pos, brightest, block));
            if (other < block) {
                normal = LichenLighting.encode(block, sky, other);
                lichenOnly = Math.max(other, sky * snapshot.daylight) < LightboundSettings.threshold(null);
            }
        }
        double drain = snapshot.drain(pos);
        if (drain > 0) {
            double effective = Math.max(0, Math.max(Math.max(block, sky * snapshot.daylight), moving) - drain);
            return TerrainFade.light(pos, effective >= LightboundSettings.threshold(null), LightCoordsUtil.pack((int)effective, 0), false);
        }
        // Select an exact lightmap texel in binary mode. Fractional packed values
        // would interpolate between the black and comfortable-light texels at a rim.
        return TerrainFade.light(pos, Math.max(Math.max(block, sky * snapshot.daylight), moving) >= LightboundSettings.threshold(null), normal, lichenOnly);
    }

    private static int addLight(int vanilla, double light) {
        int moving = (int) Math.clamp(light * 16.0, 0.0, 240.0);
        return moving > LightCoordsUtil.smoothBlock(vanilla)
                ? LightCoordsUtil.smoothPack(moving, LightCoordsUtil.smoothSky(vanilla)) : vanilla;
    }

    private static int attenuate(int packed, double drain, double daylight, boolean binary) {
        if (drain <= 0) return packed;
        double block = LightCoordsUtil.smoothBlock(packed) / 16.0;
        double sky = LightCoordsUtil.smoothSky(packed) / 16.0 * daylight;
        double effective = Math.max(0, Math.max(block, sky) - drain);
        // A drained block has one real effective value, including diminished daylight.
        // Encoding that on the block axis avoids a second sky attenuation in the shader.
        return binary ? LightCoordsUtil.pack((int)effective, 0)
                : LightCoordsUtil.smoothPack((int)(effective * 16), 0);
    }

    /** Used by renderer regression tests; the returned value is exactly the shared attenuation. */
    public static double sampleLight(BlockPos pos) {
        return current.sample(pos);
    }

    public static int sourceCount() {
        int count = 0;
        for (Source source : current.sources) if (!source.negative) count++;
        return count;
    }

    private record SourceKey(long id, boolean negative) {}

    private record Source(long id, double x, double y, double z, double luminance, double radius, boolean negative) {
        double distanceSquared(double sampleX, double sampleY, double sampleZ) {
            double dx = x - sampleX;
            double dy = y - sampleY;
            double dz = z - sampleZ;
            return dx * dx + dy * dy + dz * dz;
        }
    }

    private record Snapshot(boolean active, boolean binary, double daylight, List<Source> sources, Long2ObjectMap<Source[]> sections) {
        double sample(BlockPos pos) {
            return sample(pos, false);
        }
        double drain(BlockPos pos) {
            return sample(pos, true);
        }
        double sample(BlockPos pos, boolean negative) {
            if (sources.isEmpty()) return 0;
            Source[] local = sections.get(SectionPos.asLong(pos.getX() >> 4, pos.getY() >> 4, pos.getZ() >> 4));
            if (local == null) return 0;
            double light = 0;
            for (Source source : local) {
                if (source.negative != negative) continue;
                double distanceSquared = source.distanceSquared(pos.getX() + 0.5, pos.getY() + 0.5, pos.getZ() + 0.5);
                if (distanceSquared < source.radius * source.radius) {
                    double distance = Math.sqrt(distanceSquared);
                    light = Math.max(light, negative ? source.luminance * (1 - distance / source.radius)
                            : LightMath.dynamicLight((int)source.luminance, distance));
                }
            }
            return light;
        }
    }
}
