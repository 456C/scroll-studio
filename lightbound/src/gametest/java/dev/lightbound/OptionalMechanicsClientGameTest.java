package dev.lightbound;

import dev.lightbound.client.BuiltinDynamicLighting;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;

/** Exercises actual movement packets, airborne validation, rule sync, and trail expiry. */
@SuppressWarnings("UnstableApiUsage")
public final class OptionalMechanicsClientGameTest implements FabricClientGameTest {
    @Override
    public void runTest(ClientGameTestContext context) {
        try (var world = context.worldBuilder().create()) {
            var server = world.getServer();
            var connection = world.getConnection();
            server.runCommand("gamemode creative @p");
            server.runCommand("time set midnight");
            server.runCommand("fill -8 195 -8 8 210 8 minecraft:stone hollow");
            server.runCommand("tp @p 0.5 200 0.5 0 0");
            server.runOnServer(ms -> {
                var p = connection.getServerPlayer();
                p.getAbilities().flying = true;
                p.onUpdateAbilities();
                p.setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH));
            });
            server.runCommand("gamerule lightbound:afterglow_ticks 40");
            connection.waitForClientboundPackets();
            context.waitFor(client -> LightboundSettings.afterglowTicks(client.level) == 40
                    && BuiltinDynamicLighting.sourceCount() >= 1);
            context.waitTicks(5);
            var probe = new BlockPos(0, 200, 0);
            server.runOnServer(ms -> connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY));
            connection.waitForClientboundPackets();
            context.waitTicks(3);
            context.runOnClient(client -> {
                require(LightRules.getEffectiveLight(client.level, probe) > 1,
                        "Afterglow must retain a solid light pool after removing the torch");
                require(BuiltinDynamicLighting.sampleLight(probe) > 1,
                        "The native renderer must retain the same afterglow pool");
            });
            // Ordinary singleplayer pause freezes world-time history, even though
            // interface/client ticks continue. Wait until a server snapshot arrived.
            context.waitTicks(10);
            context.setScreen(() -> new net.minecraft.client.gui.screens.PauseScreen(true));
            context.waitFor(client -> client.isPaused());
            long pausedTick = context.computeOnClient(client -> client.level.getGameTime());
            context.waitTicks(60);
            context.runOnClient(client -> {
                require(client.level.getGameTime() == pausedTick, "Pause fixture must freeze world time");
                require(LightRules.getEffectiveLight(client.level, probe) > 1
                                && BuiltinDynamicLighting.sampleLight(probe) > 1,
                        "Paused afterglow must not expire from interface ticks");
                client.gui.setScreen(null);
            });
            context.waitTicks(45);
            context.runOnClient(client -> {
                require(LightRules.getEffectiveLight(client.level, probe) == 0,
                        "Expired afterglow must stop affecting player collision");
                require(BuiltinDynamicLighting.sampleLight(probe) == 0,
                        "Expired afterglow must be removed from the renderer");
            });

            // Leave an existing trail, replace ClientLevel twice, then return with
            // empty hands. The client cannot reconstruct this history from emitters.
            server.runCommand("gamerule lightbound:afterglow_ticks 200");
            server.runOnServer(ms -> connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH)));
            connection.waitForClientboundPackets();
            context.waitTicks(5);
            server.runOnServer(ms -> connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY));
            connection.waitForClientboundPackets();
            server.runCommand("execute as @p in minecraft:the_nether run tp @s 0.5 200 0.5");
            connection.waitForClientboundPackets();
            context.waitFor(client -> client.level.dimension().equals(net.minecraft.world.level.Level.NETHER));
            context.waitTicks(3);
            server.runCommand("execute as @a[limit=1] in minecraft:overworld run tp @s 0.5 200 0.5");
            connection.waitForClientboundPackets();
            context.waitFor(client -> client.level.dimension().equals(net.minecraft.world.level.Level.OVERWORLD));
            context.waitTicks(3);
            connection.waitForClientboundPackets();
            double authoritativeHistory = server.computeOnServer(ms -> LightRules.getEffectiveLight(connection.getServerPlayer().level(), probe));
            context.runOnClient(client -> {
                require(authoritativeHistory > 1, "The original server trail must still exist on return");
                require(Math.abs(authoritativeHistory - LightRules.getEffectiveLight(client.level, probe)) < 0.01,
                        "A returning client must receive the existing server afterglow, even with empty hands");
                require(Math.abs(authoritativeHistory - BuiltinDynamicLighting.sampleLight(probe)) < 0.01,
                        "Native lighting must render the authoritative returning trail");
            });

            server.runCommand("gamerule lightbound:afterglow_ticks 0");
            server.runCommand("gamerule lightbound:dark_swimming true");
            server.runCommand("gamemode survival @p");
            server.runCommand("tp @p 0.5 200 0.5 0 0");
            connection.waitForClientboundPackets();
            context.waitFor(client -> LightboundSettings.darkSwimming(client.level)
                    && !client.player.getAbilities().flying);
            context.runOnClient(client -> client.options.keyJump.setDown(true));
            // Vanilla normally rejects sustained unauthorized floating after eighty ticks.
            context.waitTicks(110);
            context.runOnClient(client -> client.options.keyJump.setDown(false));
            double clientY = context.computeOnClient(client -> client.player.getY());
            double serverY = server.computeOnServer(ms -> connection.getServerPlayer().getY());
            require(clientY > 205 && Math.abs(clientY - serverY) < 1,
                    "Dark swimming must rise and stay synchronized beyond the floating kick window: " + clientY + "/" + serverY);

            server.runOnServer(ms -> connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH)));
            connection.waitForClientboundPackets();
            context.waitTicks(4);
            context.runOnClient(client -> require(!LightMechanics.darkSwimmingActive(client.player),
                    "Real light must restore ordinary movement"));

            server.runCommand("gamerule lightbound:void_loop true");
            server.runOnServer(ms -> {
                var p = connection.getServerPlayer();
                p.setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY);
                p.fallDistance = 100;
                p.teleportTo(p.level(), 0.5, p.level().getMinY() - 33, 0.5, java.util.Set.of(), 0, 0, false);
            });
            context.waitFor(client -> client.player.getY() > client.level.getMaxY());
            connection.waitForClientboundPackets();
            server.runOnServer(ms -> {
                var p = connection.getServerPlayer();
                require(p.getY() > p.level().getMaxY() && p.isAlive(), "The void loop must return the live player above the same world");
                require(Math.abs(p.getX() - 0.5) < 0.1 && Math.abs(p.getZ() - 0.5) < 0.1,
                        "The void loop must preserve horizontal position");
                require(p.fallDistance < 2, "The void loop must clear accumulated fall distance");
            });
            System.out.println("LIGHTBOUND OPTIONAL MECHANICS VERIFIED native afterglow, real dark-swimming movement, and void-loop teleport");
        } finally {
            context.runOnClient(client -> client.options.keyJump.setDown(false));
        }
    }

    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
}
