package dev.lightbound;

import java.util.HashSet;
import java.util.List;

import net.fabricmc.fabric.api.gametest.v1.GameTest;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.world.level.block.Blocks;

/** Tests the world-space edge locations, including scenes that must have no glow. */
public final class VoidEdgeGameTest {
    // Asynchronous skylight updates need wall-clock time even on an accelerated test server.
    private static final int FLOOR_RADIUS = 20;
    private static final int THRESHOLD = 15;

    @GameTest(maxTicks = 1000, padding = 50)
    public void fullyLitFloorHasNoPlayerRadiusRing(GameTestHelper test) {
        BlockPos center = makeFloor(test);
        for (BlockPos viewpoint : List.of(center, center.east(3), center.north(3))) {
            var edges = scan(test, center, viewpoint);
            test.assertTrue(edges.isEmpty(), "A continuous lit floor must have no edges at viewpoint "
                    + viewpoint + ": " + edges);
        }
        test.succeed();
    }

    @GameTest(maxTicks = 1000, padding = 50)
    public void wallsAndOneBlockStepsAreNotVoid(GameTestHelper test) {
        BlockPos center = makeFloor(test);
        for (int x = -6; x <= 6; x++) {
            for (int z = -6; z <= 6; z++) {
                if (Math.abs(x) != 6 && Math.abs(z) != 6) continue;
                for (int y = 1; y <= 5; y++) {
                    test.getLevel().setBlock(center.offset(x, y, z), Blocks.GLOWSTONE.defaultBlockState(), 2);
                }
            }
        }
        for (int x = 8; x <= FLOOR_RADIUS; x++) {
            for (int z = -FLOOR_RADIUS; z <= FLOOR_RADIUS; z++) {
                test.getLevel().setBlock(center.offset(x, 1, z), Blocks.GLOWSTONE.defaultBlockState(), 2);
            }
        }
        test.startSequence().thenWaitUntil(() -> test.assertTrue(
                LightRules.getEffectiveLight(test.getLevel(), center.offset(5, 5, 0)) >= 1,
                "Wall top must face lit air")).thenExecute(() -> {
            var edges = VoidEdgeScanner.scan(test.getLevel(), center.above(), center.getY() + 1, 1.8, 1);
            test.assertTrue(edges.isEmpty(), "Lit room walls and steps must have zero lines: " + edges);
        }).thenSucceed();
    }

    @GameTest(maxTicks = 1000, padding = 50)
    public void onlyTheActualHoleRimGlows(GameTestHelper test) {
        BlockPos center = makeDarkPatch(test);
        test.startSequence().thenWaitUntil(() -> checkPatchLighting(test, center)).thenExecute(() ->
                assertHoleRim(test, center, scan(test, center, center.west(4)))).thenSucceed();
    }

    @GameTest(maxTicks = 1000, padding = 50)
    public void aFixedHoleDoesNotFollowThePlayer(GameTestHelper test) {
        BlockPos center = makeDarkPatch(test);
        test.startSequence().thenWaitUntil(() -> checkPatchLighting(test, center)).thenExecute(() -> {
            var outside = scan(test, center, center.west(4));
            var overHole = scan(test, center, center);
            assertHoleRim(test, center, outside);
            assertHoleRim(test, center, overHole);
            test.assertTrue(new HashSet<>(outside).equals(new HashSet<>(overHole)),
                    "Moving over the dark hole must preserve the same world-space perimeter");
        }).thenSucceed();
    }

    @GameTest(maxTicks = 1000, padding = 50)
    public void darkAirOverALitLandingHasNoUpperRim(GameTestHelper test) {
        BlockPos center = makeDarkPatch(test);
        putLanding(test, center);
        test.startSequence().thenWaitUntil(() -> checkPatchLighting(test, center)).thenExecute(() -> {
            var edges = scan(test, center, center.west(4));
            test.assertTrue(edges.stream().noneMatch(edge -> edge.y1() > center.getY() || edge.y2() > center.getY()),
                    "A lower lit landing must clear every segment on the upper lip: " + edges);
        }).thenSucceed();
    }

    @GameTest(maxTicks = 1000, padding = 50)
    public void phasedTerrainDirectlyBesideTheLipRetainsItsRim(GameTestHelper test) {
        BlockPos center = makeDarkPatch(test);
        putLanding(test, center);
        for (int x = -1; x <= 1; x++) {
            for (int z = -1; z <= 1; z++) {
                test.getLevel().setBlock(center.offset(x, 0, z), Blocks.STONE.defaultBlockState(), 2);
            }
        }
        test.startSequence().thenWaitUntil(() -> checkPatchLighting(test, center)).thenExecute(() -> {
            var upper = scan(test, center, center.west(4)).stream()
                    .filter(edge -> edge.y1() > center.getY() && edge.y2() > center.getY()).toList();
            assertHoleRim(test, center, upper);
        }).thenSucceed();
    }

    private static void putLanding(GameTestHelper test, BlockPos center) {
        for (int x = -1; x <= 1; x++) {
            for (int z = -1; z <= 1; z++) {
                test.getLevel().setBlock(center.offset(x, -2, z), Blocks.GLOWSTONE.defaultBlockState(), 2);
            }
        }
    }

    private static BlockPos makeFloor(GameTestHelper test) {
        BlockPos center = test.absolutePos(new BlockPos(0, 100, 0));
        // Each fixture is deliberately wider than the scanner plus its neighbour margin.
        // Loading known chunks makes an absent line different from an unknown-chunk result.
        for (int x = (center.getX() - FLOOR_RADIUS - 2) >> 4;
                x <= (center.getX() + FLOOR_RADIUS + 2) >> 4; x++) {
            for (int z = (center.getZ() - FLOOR_RADIUS - 2) >> 4;
                    z <= (center.getZ() + FLOOR_RADIUS + 2) >> 4; z++) {
                test.getLevel().getChunk(x, z);
            }
        }
        for (int x = -FLOOR_RADIUS; x <= FLOOR_RADIUS; x++) {
            for (int z = -FLOOR_RADIUS; z <= FLOOR_RADIUS; z++) {
                test.getLevel().setBlock(center.offset(x, 0, z), Blocks.GLOWSTONE.defaultBlockState(), 2);
            }
        }
        return center;
    }

    private static BlockPos makeDarkPatch(GameTestHelper test) {
        BlockPos center = makeFloor(test);
        for (int x = -8; x <= 8; x++) {
            for (int z = -8; z <= 8; z++) {
                // A roof beyond the scan keeps midday skylight out of the dark patch.
                test.getLevel().setBlock(center.offset(x, 16, z), Blocks.STONE.defaultBlockState(), 2);
                boolean hole = Math.abs(x) <= 1 && Math.abs(z) <= 1;
                test.getLevel().setBlock(center.offset(x, 0, z),
                        (hole ? Blocks.AIR : Blocks.STONE).defaultBlockState(), 2);
                if (!hole) test.getLevel().setBlock(center.offset(x, -1, z), Blocks.GLOWSTONE.defaultBlockState(), 2);
            }
        }
        return center;
    }

    private static void checkPatchLighting(GameTestHelper test, BlockPos center) {
        test.assertTrue(LightRules.getEffectiveLight(test.getLevel(), center.west(2)) >= THRESHOLD
                        && LightRules.getEffectiveLight(test.getLevel(), center.west()) < THRESHOLD
                        && LightRules.getEffectiveLight(test.getLevel(), center) < THRESHOLD,
                "Fixture needs a lit rim immediately beside a dark patch");
    }

    private static List<VoidEdgeScanner.Edge> scan(GameTestHelper test, BlockPos floor, BlockPos viewpoint) {
        return VoidEdgeScanner.scan(test.getLevel(), viewpoint.above(), floor.getY() + 1.0, 1.8, THRESHOLD);
    }

    private static void assertHoleRim(GameTestHelper test, BlockPos center, List<VoidEdgeScanner.Edge> edges) {
        test.assertTrue(edges.size() == 12, "The 3x3 hole must have exactly12 one-block rim segments, got " + edges);
        double minX = center.getX() - 1;
        double maxX = center.getX() + 2;
        double minZ = center.getZ() - 1;
        double maxZ = center.getZ() + 2;
        for (var edge : edges) {
            boolean alongXSide = close(edge.x1(), edge.x2())
                    && (close(edge.x1(), minX) || close(edge.x1(), maxX))
                    && edge.z1() >= minZ && edge.z1() <= maxZ && edge.z2() >= minZ && edge.z2() <= maxZ;
            boolean alongZSide = close(edge.z1(), edge.z2())
                    && (close(edge.z1(), minZ) || close(edge.z1(), maxZ))
                    && edge.x1() >= minX && edge.x1() <= maxX && edge.x2() >= minX && edge.x2() <= maxX;
            test.assertTrue(alongXSide || alongZSide, "Glow must lie on the actual hole boundary: " + edge);
            test.assertTrue(Math.abs(edge.y1() - (center.getY() + 1)) < 0.02
                            && Math.abs(edge.y2() - (center.getY() + 1)) < 0.02,
                    "Glow must sit on the floor surface: " + edge);
        }
    }

    private static boolean close(double first, double second) {
        return Math.abs(first - second) < 1.0e-6;
    }
}
