package dev.lightbound;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.BooleanOp;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

/** A rim needs lit footing on one side and confirmed void on the other. */
public final class VoidEdgeScanner {
    public static final int RADIUS = 12;
    public static final int MAX_SEGMENTS = 512;
    private static final int MAX_BLOCK_SAMPLES = 12288;
    private static final int LANDING_DEPTH = 3;
    private static final double EPSILON = 0.001;
    private static final double RIM_LIFT = 0.008;
    private static final Direction[] SIDES = { Direction.NORTH, Direction.SOUTH, Direction.WEST, Direction.EAST };

    private VoidEdgeScanner() {}

    public static List<Edge> scan(Level level, BlockPos origin, double feetY, double playerHeight, int threshold) {
        Probe probe = new Probe(level, playerHeight, threshold);
        List<Edge> edges = new ArrayList<>();
        for (int dx = -RADIUS; dx <= RADIUS; dx++) {
            for (int dz = -RADIUS; dz <= RADIUS; dz++) {
                int rangeSquared = RADIUS * RADIUS - dx * dx - dz * dz;
                if (rangeSquared < 0) continue;
                int x = origin.getX() + dx;
                int z = origin.getZ() + dz;
                for (double top : probe.floors(x, z, feetY, Math.sqrt(rangeSquared))) {
                    for (Direction side : SIDES) {
                        if (!probe.bordersVoid(x + side.getStepX(), z + side.getStepZ(), top)) continue;
                        double y = top + RIM_LIFT;
                        Edge edge = switch (side) {
                            case NORTH -> new Edge(x, y, z, x + 1, y, z);
                            case SOUTH -> new Edge(x, y, z + 1, x + 1, y, z + 1);
                            case WEST -> new Edge(x, y, z, x, y, z + 1);
                            case EAST -> new Edge(x + 1, y, z, x + 1, y, z + 1);
                            default -> throw new IllegalStateException();
                        };
                        edges.add(edge);
                        if (edges.size() == MAX_SEGMENTS) return List.copyOf(edges);
                    }
                }
            }
        }
        return EdgeContours.joinSteps(edges, probe::isVoidSeam);
    }

    private static final class Probe {
        private final Level level;
        private final LightRules.LightSampler lights;
        private final double height;
        private final int threshold;
        private final Map<Long, Sample> samples = new HashMap<>();

        Probe(Level level, double height, int threshold) {
            this.level = level;
            this.height = height;
            this.threshold = threshold;
            lights = LightRules.createSampler(level);
        }

        Sample sample(int x, int y, int z) {
            if (y < level.getMinY()) return Sample.DARK_AIR;
            if (y > level.getMaxY()) return Sample.LIT_AIR;
            long key = BlockPos.asLong(x, y, z);
            Sample cached = samples.get(key);
            if (cached != null) return cached;
            BlockPos pos = new BlockPos(x, y, z);
            if (samples.size() >= MAX_BLOCK_SAMPLES || !level.isLoaded(pos)) return null;
            BlockState state = level.getBlockState(pos);
            VoxelShape shape = state.getCollisionShape(level, pos, CollisionContext.empty());
            var fluid = state.getFluidState();
            Sample result = Sample.UNCHECKED_AIR;
            if (!shape.isEmpty() || !fluid.isEmpty()) {
                double fluidTop = fluid.isEmpty() ? Double.NEGATIVE_INFINITY : fluid.getHeight(level, pos);
                result = new Sample(shape, fluidTop, LightMath.isSolid(lights.sample(pos), threshold));
            }
            samples.put(key, result);
            return result;
        }

        // Most air cells only establish headroom. Read their light only when an edge actually faces one.
        boolean isDark(int x, int y, int z, Sample sample) {
            if (sample == null) return false;
            if (sample != Sample.UNCHECKED_AIR) return !sample.lit;
            BlockPos pos = new BlockPos(x, y, z);
            boolean lit = LightMath.isSolid(lights.sample(pos), threshold);
            samples.put(pos.asLong(), lit ? Sample.LIT_AIR : Sample.DARK_AIR);
            return !lit;
        }

        List<Double> floors(int x, int z, double feetY, double verticalRange) {
            List<Double> tops = new ArrayList<>();
            for (int y = (int)Math.floor(feetY + verticalRange); y >= (int)Math.floor(feetY - verticalRange); y--) {
                Sample sample = sample(x, y, z);
                if (sample == null) break;
                if (!sample.lit || sample.shape.isEmpty()) continue;
                double top = y + sample.shape.max(Direction.Axis.Y, 0.5, 0.5);
                if (!Double.isFinite(top) || top > feetY + verticalRange || top < feetY - verticalRange) continue;
                AABB bounds = sample.shape.bounds();
                if (bounds.getXsize() < 0.75 || bounds.getZsize() < 0.75) continue;
                if (hasHeadroom(x, z, top)) tops.add(top);
            }
            return tops;
        }

        boolean hasHeadroom(int x, int z, double top) {
            AABB body = new AABB(x + 0.2, top + EPSILON, z + 0.2,
                    x + 0.8, top + height - EPSILON, z + 0.8);
            for (int y = (int)Math.floor(body.minY); y <= (int)Math.floor(body.maxY); y++) {
                Sample sample = sample(x, y, z);
                if (sample == null) return false;
                if (!sample.lit || sample.shape.isEmpty()) continue;
                if (sample.shape == Shapes.block() || Shapes.joinIsNotEmpty(
                        sample.shape.move(x, y, z), Shapes.create(body), BooleanOp.AND)) return false;
            }
            return true;
        }

        boolean bordersVoid(int x, int z, double top) {
            int lipY = (int)Math.floor(top - EPSILON);
            Sample beside = sample(x, lipY, z);
            if (beside == null) return false;
            double besideTop = lipY + beside.surface();
            if (beside.lit && besideTop >= top - EPSILON) return false;
            Sample above = sample(x, (int)Math.floor(top + EPSILON), z);
            if (above == null || (above.lit && !above.shape.isEmpty())) return false;

            // Compare terrain across an ordinary step at its own height. Air above a
            // phased lower step can receive light even though the ground there is void.
            if (besideTop >= top - EPSILON) return true;

            for (int y = lipY; y >= lipY - LANDING_DEPTH; y--) {
                Sample below = sample(x, y, z);
                if (below == null) return false;
                double surface = y + below.surface();
                if (surface < top - LANDING_DEPTH - EPSILON) continue;
                if (below.lit) return false;
                if (surface >= top - 1 - EPSILON) return true;
                break;
            }
            // Across a deeper drop the lip must border dark space. A sunlit cliff
            // or a bright tree canopy is just terrain, regardless of its height.
            return isDark(x, lipY, z, beside);
        }

        // Coincident endpoints alone do not prove a vertical line is a void boundary.
        boolean isVoidSeam(Edge seam) {
            int x = (int)seam.x1(), z = (int)seam.z1();
            int bottom = (int)Math.floor(seam.y1() - RIM_LIFT + EPSILON);
            int top = (int)Math.ceil(seam.y2() - RIM_LIFT - EPSILON);
            for (int y = bottom; y < top; y++) {
                boolean touchesVoid = false;
                for (int dx = -1; dx <= 0 && !touchesVoid; dx++) {
                    for (int dz = -1; dz <= 0 && !touchesVoid; dz++) {
                        Sample inside = sample(x + dx, y, z + dz);
                        if (inside == null || !inside.lit || inside.shape != Shapes.block()) continue;
                        int acrossX = x - 1 - dx, acrossZ = z - 1 - dz;
                        touchesVoid = isDark(acrossX, y, z + dz, sample(acrossX, y, z + dz))
                                || isDark(x + dx, y, acrossZ, sample(x + dx, y, acrossZ));
                    }
                }
                if (!touchesVoid) return false;
            }
            return true;
        }
    }

    private record Sample(VoxelShape shape, double fluidTop, boolean lit) {
        static final Sample UNCHECKED_AIR = new Sample(Shapes.empty(), Double.NEGATIVE_INFINITY, false);
        static final Sample DARK_AIR = new Sample(Shapes.empty(), Double.NEGATIVE_INFINITY, false);
        static final Sample LIT_AIR = new Sample(Shapes.empty(), Double.NEGATIVE_INFINITY, true);

        double surface() {
            return shape.isEmpty() ? fluidTop : Math.max(fluidTop, shape.max(Direction.Axis.Y, 0.5, 0.5));
        }
    }
    public record Edge(double x1, double y1, double z1, double x2, double y2, double z2) {}
}
