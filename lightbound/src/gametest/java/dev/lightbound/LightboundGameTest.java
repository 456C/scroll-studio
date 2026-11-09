package dev.lightbound;

import java.util.function.Consumer;

import net.fabricmc.fabric.api.gametest.v1.GameTest;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.MoverType;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.StairBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.CollisionContext;

/** Runs against transformed Minecraft classes, so a missing Mixin fails these tests. */
public final class LightboundGameTest {
    // The test server accelerates ticks; allow asynchronous chunk lighting to finish under load.
    private static final BlockPos FLOOR = new BlockPos(3, 1, 3);

    @GameTest(maxTicks = 1000, padding = 12)
    public void darkFloorActuallyLetsPlayerFall(GameTestHelper test) {
        inDarkRoom(test, player -> {
            BlockPos floor = test.absolutePos(FLOOR);
            test.assertTrue(LightRules.getEffectiveLight(test.getLevel(), floor) < 1,
                    "The sealed chamber must really be dark");
            test.assertFalse(test.getLevel().getBlockState(floor).getCollisionShape(
                            test.getLevel(), floor, CollisionContext.placementContext(player)).isEmpty(),
                    "Placement must still check occupancy even when player movement can phase");
            double startY = player.getY();
            player.move(MoverType.SELF, new Vec3(0, -0.4, 0));
            test.assertTrue(player.getY() < startY - 0.3,
                    "A survival player must move through an unlit stone floor");
            test.assertBlockPresent(Blocks.STONE, FLOOR);
        });
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void torchInEitherHandHoldsPlayerUp(GameTestHelper test) {
        inDarkRoom(test, player -> {
            player.setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH));
            double startY = player.getY();
            player.move(MoverType.SELF, new Vec3(0, -0.4, 0));
            test.assertTrue(Math.abs(player.getY() - startY) < 1.0e-6,
                    "An offhand torch must make the dark floor support its carrier");
            player.setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY);
            player.setItemSlot(EquipmentSlot.MAINHAND, new ItemStack(Items.TORCH));
            test.assertFalse(shapeEmpty(test, player), "The main hand must also count");
            player.setItemSlot(EquipmentSlot.MAINHAND, ItemStack.EMPTY);
            test.assertTrue(shapeEmpty(test, player), "Putting away torch; light="+LightRules.getEffectiveLight(test.getLevel(),test.absolutePos(FLOOR))+", players="+test.getLevel().players().size());
        });
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void serverThresholdChangesActualCollision(GameTestHelper test) {
        inDarkRoom(test, player -> {
            player.setItemSlot(EquipmentSlot.MAINHAND, new ItemStack(Items.TORCH));
            test.assertFalse(shapeEmpty(test, player), "Default threshold should accept a nearby torch");
            test.getLevel().getGameRules().set(LightboundSettings.MINIMUM_LIGHT, 15, test.getLevel().getServer());
            test.assertTrue(shapeEmpty(test, player), "Threshold 15 must reject attenuated torch light");
            test.setBlock(FLOOR, Blocks.GLOWSTONE);
            test.assertFalse(shapeEmpty(test, player), "A level-15 emitter must satisfy threshold 15");
        });
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void mobsAndDroppedItemsKeepNormalPhysics(GameTestHelper test) {
        inDarkRoom(test, player -> {
            BlockPos absolute = test.absolutePos(FLOOR);
            BlockState floor = test.getLevel().getBlockState(absolute);
            var zombie = test.spawnWithNoFreeWill(EntityTypes.ZOMBIE, 3.5f, 2.0f, 3.5f);
            var item = test.spawnItem(Items.STONE, new Vec3(3.5, 2.0, 3.5));
            test.assertFalse(floor.getCollisionShape(test.getLevel(), absolute, CollisionContext.of(zombie)).isEmpty(),
                    "Dark terrain must retain zombie collision");
            test.assertFalse(floor.getCollisionShape(test.getLevel(), absolute, CollisionContext.of(item)).isEmpty(),
                    "Dark terrain must retain dropped-item collision");
            double zombieY = zombie.getY();
            double itemY = item.getY();
            zombie.move(MoverType.SELF, new Vec3(0, -0.4, 0));
            item.move(MoverType.SELF, new Vec3(0, -0.4, 0));
            test.assertTrue(Math.abs(zombie.getY() - zombieY) < 1.0e-6, "The zombie must stay on the floor");
            test.assertTrue(Math.abs(item.getY() - itemY) < 1.0e-6, "The item must stay on the floor");
        });
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void relightingDoesNotTrapAPlayerAlreadyInside(GameTestHelper test) {
        inDarkRoom(test, player -> {
            BlockPos absolute = test.absolutePos(FLOOR);
            player.setPos(absolute.getX() + 0.5, absolute.getY() - 0.75, absolute.getZ() + 0.5);
            player.setItemSlot(EquipmentSlot.MAINHAND, new ItemStack(Items.TORCH));
            test.assertTrue(LightRules.getEffectiveLight(test.getLevel(), absolute) > 1,
                    "The overlap test block must be illuminated");
            test.assertTrue(shapeEmpty(test, player), "Relighting an occupied block must preserve an exit");
            test.assertFalse(player.isInWall(), "Phased geometry must not suffocate the player");
            player.setPos(absolute.getX() + 1.5, absolute.getY() + 1.1, absolute.getZ() + 0.5);
            test.assertFalse(shapeEmpty(test, player), "Collision after exit; escaping="+LightRules.isEscapingTerrain(player)+", bounds="+player.getBoundingBox());
        });
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void litSlabsAndStairsRemainWalkable(GameTestHelper test) {
        inDarkRoom(test, player -> {
            BlockPos absolute = test.absolutePos(FLOOR);
            player.setItemSlot(EquipmentSlot.MAINHAND, new ItemStack(Items.TORCH));
            test.setBlock(FLOOR, Blocks.STONE_SLAB);
            player.setPos(absolute.getX() + 0.5, absolute.getY() + 0.5, absolute.getZ() + 0.5);
            double slabY = player.getY();
            player.move(MoverType.SELF, new Vec3(0, -0.2, 0));
            test.assertTrue(Math.abs(player.getY() - slabY) < 1.0e-6,
                    "Standing inside the slab's empty half must not trigger overlap grace");

            test.setBlock(FLOOR, Blocks.STONE_STAIRS.defaultBlockState().setValue(StairBlock.FACING, Direction.NORTH));
            // A north-facing stair's lower tread is on its south half.
            player.setPos(absolute.getX() + 0.5, absolute.getY() + 0.5, absolute.getZ() + 0.85);
            double stairY = player.getY();
            player.move(MoverType.SELF, new Vec3(0, -0.2, 0));
            test.assertTrue(Math.abs(player.getY() - stairY) < 1.0e-6,
                    "The empty part above a lower stair tread must not cancel support");
        });
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void bedrockAndBarriersAlsoPhase(GameTestHelper test) {
        inDarkRoom(test, player -> {
            test.setBlock(FLOOR, Blocks.BEDROCK);
            test.assertTrue(shapeEmpty(test, player), "Unlit bedrock must also phase");
            test.setBlock(FLOOR, Blocks.BARRIER);
            test.assertTrue(shapeEmpty(test, player), "Unlit barriers must also phase");
        });
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void escapeCrossesAdjacentRelitBlocksWithoutSnagging(GameTestHelper test) {
        inDarkRoom(test, player -> {
            BlockPos floor = test.absolutePos(FLOOR);
            test.setBlock(FLOOR.east(), Blocks.STONE);
            player.setPos(floor.getX()+0.5, floor.getY()+0.1, floor.getZ()+0.5);
            player.setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH));
            double start = player.getX();
            for (int i=0; i<5; i++) player.move(MoverType.SELF, new Vec3(0.4,0,0));
            test.assertTrue(player.getX() > start+1.99, "Relit neighbouring blocks must not snag an escaping player");
            test.assertFalse(LightRules.isEscapingTerrain(player), "Escape ends once the entire body is in open air");
        });
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void darkWaterHasNoSwimmingCurrentOrEyeContact(GameTestHelper test) {
        inDarkRoom(test, player -> {
            test.setBlock(FLOOR.above(), Blocks.WATER);
            test.setBlock(FLOOR.above(2), Blocks.WATER);
            var tracker = new net.minecraft.world.entity.EntityFluidInteraction(java.util.Set.of(net.minecraft.tags.FluidTags.WATER));
            test.assertFalse(tracker.update(player, false), "Dark water must not register as fluid contact");
            test.assertFalse(tracker.isEyeInFluid(net.minecraft.tags.FluidTags.WATER), "Dark water must not drown the player");
            Vec3 before = player.getDeltaMovement();
            tracker.applyCurrentTo(net.minecraft.tags.FluidTags.WATER, player, 0.014);
            test.assertTrue(player.getDeltaMovement().equals(before), "Dark water must apply no current");
            player.setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.LANTERN));
            test.assertTrue(tracker.update(player, false), "Illuminated water must behave normally");
            test.assertTrue(tracker.isEyeInFluid(net.minecraft.tags.FluidTags.WATER), "Illuminated water restores eye submersion");
        });
    }

    private static boolean shapeEmpty(GameTestHelper test, ServerPlayer player) {
        BlockPos pos = test.absolutePos(FLOOR);
        return test.getLevel().getBlockState(pos)
                .getCollisionShape(test.getLevel(), pos, CollisionContext.of(player)).isEmpty();
    }

    @SuppressWarnings("removal")
    private static void inDarkRoom(GameTestHelper test, Consumer<ServerPlayer> assertions) {
        for (int x = 0; x < 8; x++) {
            for (int y = 0; y < 8; y++) {
                for (int z = 0; z < 8; z++) {
                    test.setBlock(x, y, z, (x == 0 || x == 7 || y == 0 || y == 7 || z == 0 || z == 7) ? Blocks.STONE : Blocks.AIR);
                }
            }
        }
        test.setBlock(FLOOR, Blocks.STONE);
        // Let the real light engine finish the opaque chamber's skylight update.
        test.startSequence().thenWaitUntil(() -> test.assertTrue(
                LightRules.getEffectiveLight(test.getLevel(), test.absolutePos(FLOOR)) < 1,
                "Waiting for the real light engine to darken the chamber")).thenExecute(() -> {
            int previousThreshold = LightboundSettings.threshold(test.getLevel());
            test.getLevel().getGameRules().set(LightboundSettings.MINIMUM_LIGHT, 1, test.getLevel().getServer());
            ServerPlayer player = test.makeMockServerPlayerInLevel();
            try {
                player.setGameMode(GameType.SURVIVAL);
                BlockPos floor = test.absolutePos(FLOOR);
                player.setPos(floor.getX() + 0.5, floor.getY() + 1.0, floor.getZ() + 0.5);
                assertions.accept(player);
            } finally {
                test.getLevel().getServer().getPlayerList().remove(player);
                test.getLevel().getGameRules().set(LightboundSettings.MINIMUM_LIGHT,
                        previousThreshold, test.getLevel().getServer());
            }
        }).thenSucceed();
    }
}
