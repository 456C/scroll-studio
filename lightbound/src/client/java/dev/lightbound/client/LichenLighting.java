package dev.lightbound.client;

import it.unimi.dsi.fastutil.ints.Int2ByteOpenHashMap;
import dev.lightbound.client.mixin.RenderSectionRegionAccessor;
import dev.lightbound.client.mixin.SectionCopyAccessor;
import java.lang.ref.WeakReference;
import net.minecraft.client.renderer.chunk.RenderSectionRegion;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.util.LightCoordsUtil;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.lighting.LightEngine;

/** Separates glow lichen from other block light while the terrain mesh is built. */
public final class LichenLighting {
    static final int EMISSION = 7;
    static final int GRADIENT = 1 << 28;
    private static final Direction[] SIDES = Direction.values();
    private static final ThreadLocal<LichenLighting> WORKER = ThreadLocal.withInitial(LichenLighting::new);
    private WeakReference<RenderSectionRegion> region = new WeakReference<>(null);
    private boolean hasLichen;
    private final Int2ByteOpenHashMap cache = new Int2ByteOpenHashMap();

    private LichenLighting() {
        cache.defaultReturnValue((byte)-1);
    }

    static LichenLighting forRegion(RenderSectionRegion world) {
        LichenLighting worker = WORKER.get();
        if (worker.region.get() != world) {
            worker.cache.clear();
            worker.region = new WeakReference<>(world);
            worker.hasLichen = false;
            for (var section : ((RenderSectionRegionAccessor)world).lightbound$sections()) {
                if (section == null) continue;
                var states = ((SectionCopyAccessor)section).lightbound$states();
                if (states != null && states.maybeHas(state -> state.is(Blocks.GLOW_LICHEN))) {
                    worker.hasLichen = true;
                    break;
                }
            }
        }
        return worker.hasLichen ? worker : null;
    }

    int otherLight(RenderSectionRegion world, BlockPos block, BlockPos brightest, int level) {
        // One non-lichen source supplying the maximum is enough to keep the original light.
        if (!leadsToLichen(world, brightest, level)) return level;
        int best = withoutLichen(world, block, 1);
        for (Direction side : SIDES) {
            if (best >= level) break;
            best = Math.max(best, withoutLichen(world, block.relative(side), best + 1));
        }
        return best;
    }

    // Smooth lighting blends face, edge and diagonal samples around a block.
    // A block with no face light can still receive a fractional corner highlight.
    int gradientLight(RenderSectionRegion world, BlockPos block, double moving) {
        int other = (int)moving;
        int brightest = 0;
        BlockPos.MutableBlockPos cell = new BlockPos.MutableBlockPos();
        for (int x = -1; x <= 1; x++) {
            for (int y = -1; y <= 1; y++) {
                for (int z = -1; z <= 1; z++) {
                    cell.setWithOffset(block, x, y, z);
                    int key = key(cell, 9);
                    int sample = cache.get(key);
                    if (sample < 0) {
                        int raw = world.getBrightness(LightLayer.BLOCK, cell);
                        if (raw > EMISSION) return -1;
                        int independent = raw > 0 && leadsToLichen(world, cell, raw) ? withoutLichen(world, cell, 1) : raw;
                        sample = (raw << 3) | independent;
                        cache.put(key, (byte)sample);
                    }
                    brightest = Math.max(brightest, sample >> 3);
                    other = Math.max(other, sample & 7);
                }
            }
        }
        return brightest > other ? GRADIENT | LightCoordsUtil.smoothPack(0, other) : -1;
    }

    // A section is 16 cells wide. Face sampling and at most seven propagation steps
    // fit within its 32-cell-wide window, including at negative world coordinates.
    private static int key(BlockPos p, int minimum) {
        return (p.getX() & 31) | ((p.getY() & 31) << 5) | ((p.getZ() & 31) << 10) | (minimum << 15);
    }

    private boolean leadsToLichen(RenderSectionRegion world, BlockPos pos, int level) {
        int key = key(pos, 0);
        int known = cache.get(key);
        if (known >= 0) return known != 0;
        BlockState state = world.getBlockState(pos);
        boolean lichen = state.is(Blocks.GLOW_LICHEN) && state.getLightEmission() >= level;
        int loss = Math.max(1, state.getLightDampening());
        if (state.getLightEmission() < level && level + loss <= EMISSION) {
            for (Direction side : SIDES) {
                BlockPos next = pos.relative(side);
                int nextLight = world.getBrightness(LightLayer.BLOCK, next);
                if (nextLight < level + loss) continue;
                if (LightEngine.getLightDampeningInto(world.getBlockState(next), state, side.getOpposite(), loss) >= 16) continue;
                lichen = nextLight <= EMISSION && leadsToLichen(world, next, nextLight);
                break;
            }
        }
        cache.put(key, (byte)(lichen ? 1 : 0));
        return lichen;
    }

    // Recover weaker overlapping sources too. Each step raises the required light.
    // Anything above seven already comes entirely from another emitter.
    private int withoutLichen(RenderSectionRegion world, BlockPos pos, int minimum) {
        int measured = world.getBrightness(LightLayer.BLOCK, pos);
        if (measured < minimum) return 0;
        if (measured > EMISSION) return measured;
        int key = key(pos, minimum);
        int cached = cache.get(key);
        if (cached >= 0) return cached;
        BlockState state = world.getBlockState(pos);
        int best = state.is(Blocks.GLOW_LICHEN) ? 0 : state.getLightEmission();
        int loss = Math.max(1, state.getLightDampening());
        for (Direction side : SIDES) {
            if (best >= measured || loss >= 15) break;
            int required = Math.max(minimum, best + 1) + loss;
            BlockPos next = pos.relative(side);
            if (world.getBrightness(LightLayer.BLOCK, next) < required) continue;
            if (LightEngine.getLightDampeningInto(world.getBlockState(next), state, side.getOpposite(), loss) >= 16) continue;
            best = Math.max(best, withoutLichen(world, next, required) - loss);
        }
        int result = best < minimum ? 0 : Math.min(measured, best);
        cache.put(key, (byte)result);
        return result;
    }

    // Vanilla light coordinates stop at 240. The sky nibble remains available
    // for the real sky level, so day/night and night vision update without a rebake.
    static int encode(int block, int sky, int other) {
        return LightCoordsUtil.smoothPack(240 + block, sky * 16 + other);
    }

    public static int vertexLight(int terrain, int vertex) {
        if ((terrain & GRADIENT) == 0) return terrain;
        int smooth = LightCoordsUtil.smoothBlock(vertex);
        int other = LightCoordsUtil.smoothSky(terrain) & 15;
        if (smooth <= other * 16 || smooth > EMISSION * 16) return vertex;
        int halfSteps = Math.max(1, Math.round(smooth / 8.0f));
        int marker = (halfSteps & 1) == 0 ? 240 + halfSteps / 2 : 248 + halfSteps / 2;
        return LightCoordsUtil.smoothPack(marker, LightCoordsUtil.sky(vertex) * 16 + other);
    }

    public static boolean usesVertexLight(int terrain) {
        return (terrain & GRADIENT) != 0;
    }
}
