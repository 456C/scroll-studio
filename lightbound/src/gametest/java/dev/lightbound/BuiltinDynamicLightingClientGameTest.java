package dev.lightbound;

import dev.lightbound.client.BuiltinDynamicLighting;
import dev.lightbound.client.ClientVisualSettings;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.minecraft.core.BlockPos;
import net.minecraft.client.renderer.block.BlockAndTintGetter;
import net.minecraft.client.renderer.block.FluidRenderer;
import net.minecraft.util.LightCoordsUtil;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.LightLayer;

/** Regression coverage for the native renderer path, without another lighting mod. */
@SuppressWarnings("UnstableApiUsage")
public final class BuiltinDynamicLightingClientGameTest implements FabricClientGameTest {
    @Override
    public void runTest(ClientGameTestContext context) {
        boolean enabled = context.computeOnClient(client -> ClientVisualSettings.dynamicLighting);
        boolean binary = context.computeOnClient(client -> ClientVisualSettings.binaryLighting);
        try (TestSingleplayerContext singleplayer = context.worldBuilder().create()) {
            var server = singleplayer.getServer();
            var connection = singleplayer.getConnection();
            server.runCommand("gamerule lightbound:afterglow_ticks 0");
            context.runOnClient(client -> ClientVisualSettings.dynamicLighting = true);
            server.runCommand("gamemode creative @p");
            server.runCommand("fill -24 95 -8 -8 103 8 minecraft:stone hollow");
            server.runCommand("tp @p -16.5 96 0.5 0 0");
            server.runCommand("time set midnight");
            server.runOnServer(ms -> {
                var player = connection.getServerPlayer();
                player.getAbilities().flying = true;
                player.onUpdateAbilities();
                player.setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH));
            });
            connection.waitForClientboundPackets();
            context.waitFor(client -> BuiltinDynamicLighting.sourceCount() == 1);
            context.waitTicks(4);

            // The emitter overlaps two negative-coordinate sections. Both sides must work.
            assertParity(context, new BlockPos(-17, 95, 0));
            assertParity(context, new BlockPos(-16, 95, 0));
            double beforeMove = context.computeOnClient(client -> BuiltinDynamicLighting.sampleLight(new BlockPos(-23, 97, 0)));
            if (beforeMove <= 0) throw new AssertionError("Old torch radius is missing before movement");

            server.runCommand("tp @p -13.5 96 0.5 0 0");
            connection.waitForClientboundPackets();
            context.waitTicks(4);
            assertParity(context, new BlockPos(-13, 95, 0));
            double afterMove = context.computeOnClient(client -> BuiltinDynamicLighting.sampleLight(new BlockPos(-23, 97, 0)));
            if (afterMove != 0) throw new AssertionError("Old torch radius remained lit after movement: " + afterMove);

            // Both hands use the same maximum luminance as physical solidity.
            server.runOnServer(ms -> {
                var player = connection.getServerPlayer();
                player.setItemSlot(EquipmentSlot.MAINHAND, new ItemStack(Items.TORCH));
                player.setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.LANTERN));
            });
            connection.waitForClientboundPackets();
            context.waitTicks(4);
            assertParity(context, new BlockPos(-13, 95, 0));

            server.runOnServer(ms -> connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY));
            server.runCommand("fill -15 96 -1 -12 99 1 minecraft:water");
            connection.waitForClientboundPackets();
            context.waitFor(client -> LightRules.heldLuminance(client.player) == 0 && BuiltinDynamicLighting.sourceCount() == 0);
            assertUnlit(context, new BlockPos(-13, 95, 0));

            // Lanterns keep working underwater, while regular torches extinguish.
            server.runOnServer(ms -> connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.LANTERN)));
            connection.waitForClientboundPackets();
            context.waitFor(client -> BuiltinDynamicLighting.sourceCount() == 1);
            context.waitTicks(4);
            assertParity(context, new BlockPos(-13, 95, 0));

            context.runOnClient(client -> ClientVisualSettings.dynamicLighting = false);
            context.waitFor(client -> BuiltinDynamicLighting.sourceCount() == 0);
            assertUnlit(context, new BlockPos(-13, 95, 0));
            context.runOnClient(client -> {
                if (LightRules.getEffectiveLight(client.level, new BlockPos(-13, 95, 0)) < 1) {
                    throw new AssertionError("A visual setting changed physical light");
                }
                ClientVisualSettings.dynamicLighting = true;
            });
            context.waitFor(client -> BuiltinDynamicLighting.sourceCount() == 1);
            server.runOnServer(ms -> {
                connection.getServerPlayer().setItemSlot(EquipmentSlot.MAINHAND, ItemStack.EMPTY);
                connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY);
            });
            connection.waitForClientboundPackets();
            context.waitFor(client -> BuiltinDynamicLighting.sourceCount() == 0);
            assertUnlit(context, new BlockPos(-13, 95, 0));
        } finally {
            context.runOnClient(client -> {
                ClientVisualSettings.dynamicLighting = enabled;
                ClientVisualSettings.binaryLighting = binary;
            });
        }
    }

    private static void assertParity(ClientGameTestContext context, BlockPos probe) {
        context.runOnClient(client -> {
            var player = client.player;
            double dx = player.getX() - probe.getX() - 0.5;
            double dy = player.getEyeY() - probe.getY() - 0.5;
            double dz = player.getZ() - probe.getZ() - 0.5;
            double expected = LightMath.dynamicLight(LightRules.heldLuminance(player), Math.sqrt(dx * dx + dy * dy + dz * dz));
            double actual = BuiltinDynamicLighting.sampleLight(probe);
            if (Math.abs(expected - actual) > 0.00001) {
                throw new AssertionError("Moving light differs from physics at " + probe + ": " + expected + " vs " + actual);
            }
            int coords = LightCoordsUtil.getLightCoords(client.level, probe);
            if (LightCoordsUtil.smoothBlock(coords) != (int) (actual * 16)) {
                throw new AssertionError("Native light-coordinate renderer hook did not apply at " + probe);
            }
            int binary = BuiltinDynamicLighting.blockLightCoords(client.level, client.level.getBlockState(probe), probe);
            if (LightCoordsUtil.smoothBlock(binary) != (int) actual * 16) {
                throw new AssertionError("Binary lighting must choose a single exact lightmap texel");
            }
            if (client.level.getBrightness(LightLayer.BLOCK, probe) != 0) {
                throw new AssertionError("Client light was written into the world light engine");
            }
        });
    }

    private static void assertUnlit(ClientGameTestContext context, BlockPos probe) {
        context.runOnClient(client -> {
            if (BuiltinDynamicLighting.sampleLight(probe) != 0
                    || LightCoordsUtil.smoothBlock(LightCoordsUtil.getLightCoords(client.level, probe)) != 0) {
                throw new AssertionError("A removed/disabled torch left stale client light");
            }
        });
    }
}
