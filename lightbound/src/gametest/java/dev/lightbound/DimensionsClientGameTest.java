package dev.lightbound;

import dev.lightbound.client.AuraParticles;
import dev.lightbound.client.BuiltinDynamicLighting;
import dev.lightbound.client.ClientVisualSettings;
import java.awt.image.BufferedImage;
import java.io.IOException;
import java.nio.file.Path;
import javax.imageio.ImageIO;
import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.fabricmc.fabric.api.client.gametest.v1.screenshot.TestScreenshotOptions;
import net.minecraft.core.BlockPos;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.phys.shapes.CollisionContext;

/** Real renderer/connection regression tests across Nether and End level replacements. */
@SuppressWarnings("UnstableApiUsage")
public final class DimensionsClientGameTest implements FabricClientGameTest {
    private static final double BRIGHTNESS = 0.72;
    private static final int THRESHOLD = 3;

    @Override
    public void runTest(ClientGameTestContext context) {
        Settings saved = context.computeOnClient(client -> Settings.capture(client.gui.hud.isHidden()));
        try (TestSingleplayerContext singleplayer = context.worldBuilder().create()) {
            var server = singleplayer.getServer();
            var connection = singleplayer.getConnection();
            server.runCommand("gamerule lightbound:afterglow_ticks 0");
            context.runOnClient(client -> {
                ClientVisualSettings.dynamicLighting = true;
                ClientVisualSettings.binaryLighting = true;
                ClientVisualSettings.gatewayVoid = false;
                ClientVisualSettings.edgeGlow = true;
                ClientVisualSettings.auraParticles = true;
                ClientVisualSettings.glowStrength = 1;
                ClientVisualSettings.particleDensity = 1;
                ClientVisualSettings.litBrightness = BRIGHTNESS;
                ClientVisualSettings.revision++;
            });
            server.runCommand("gamemode creative @a");
            server.runCommand("gamerule lightbound:minimum_light " + THRESHOLD);
            server.runCommand("time set midnight");
            server.runCommand("effect clear @a");
            server.runOnServer(ms -> {
                var player = connection.getServerPlayer();
                player.getAbilities().flying = true;
                player.onUpdateAbilities();
                player.setItemSlot(EquipmentSlot.MAINHAND, ItemStack.EMPTY);
                player.setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH));
            });
            connection.waitForClientboundPackets();
            context.waitFor(client -> BuiltinDynamicLighting.sourceCount() == 1);
            BlockPos overworldProbe = context.computeOnClient(client -> client.player.blockPosition());

            clearHeldLight(singleplayer);
            visit(context, singleplayer, Level.NETHER, "minecraft:the_nether", "nether", 160, overworldProbe);
            checkPlacedLava(context, singleplayer);

            // Seed visible, real boundary particles before replacing ClientLevel again.
            server.runCommand("execute in minecraft:the_nether run fill 2 160 -1 3 160 1 minecraft:air");
            server.runOnServer(ms -> connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH)));
            context.runOnClient(client -> { if (client.gui.hud.isHidden()) client.gui.hud.toggle(); });
            settle(context, singleplayer);
            context.waitFor(client -> AuraParticles.liveCount() > 0);
            BlockPos netherProbe = new BlockPos(0, 161, 0);
            clearHeldLight(singleplayer);

            visit(context, singleplayer, Level.END, "minecraft:the_end", "end", 100, netherProbe);
            System.out.println("LIGHTBOUND DIMENSIONS VERIFIED Nether/End: threshold, native light, darkness, gateway, and transition cleanup");
        } finally {
            context.runOnClient(client -> {
                saved.restore();
                if (client.gui.hud.isHidden() != saved.hudHidden) client.gui.hud.toggle();
            });
        }
    }

    private static void visit(ClientGameTestContext context, TestSingleplayerContext singleplayer,
            ResourceKey<Level> dimension, String dimensionId, String label, int floorY, BlockPos previousProbe) {
        var server = singleplayer.getServer();
        var connection = singleplayer.getConnection();
        // Select the player globally before switching command dimensions.
        server.runCommand("execute as @a[limit=1] in " + dimensionId + " run tp @s 0.5 " + (floorY + 1) + " 0.5 0 15");
        connection.waitForClientboundPackets();
        context.waitFor(client -> client.level != null && client.level.dimension().equals(dimension));
        context.waitTicks(20);
        connection.waitForClientboundPackets();
        connection.waitForChunksRender();
        server.runCommand("execute in " + dimensionId + " run fill -5 " + floorY + " -5 5 " + (floorY + 8) + " 5 minecraft:white_concrete hollow");
        server.runOnServer(ms -> {
            var player = connection.getServerPlayer();
            player.getAbilities().flying = true;
            player.onUpdateAbilities();
        });
        settle(context, singleplayer);
        context.runOnClient(client -> {
            if (!LightboundSettings.isActive(client.level) || LightboundSettings.threshold(client.level) != THRESHOLD) {
                throw new AssertionError(label + " lost the server's active light threshold during transition");
            }
            if (BuiltinDynamicLighting.sourceCount() != 0 || BuiltinDynamicLighting.sampleLight(previousProbe) != 0) {
                throw new AssertionError(label + " retained the previous dimension's moving light snapshot");
            }
            if (AuraParticles.liveCount() != 0) throw new AssertionError(label + " retained old aura particles");
            if (!ClientVisualSettings.dynamicLighting || !ClientVisualSettings.binaryLighting
                    || Math.abs(ClientVisualSettings.litBrightness - BRIGHTNESS) > 0.00001) {
                throw new AssertionError(label + " reset local appearance settings");
            }
            var lightmap = client.gameRenderer.gameRenderState().lightmapRenderState;
            if (lightmap.ambientColor.x() >= 0 || Math.abs(lightmap.ambientColor.y() - THRESHOLD) > 0.00001
                    || Math.abs(lightmap.ambientColor.z() - BRIGHTNESS) > 0.00001) {
                throw new AssertionError(label + " did not apply the current binary lighting settings to its lightmap");
            }
            if (!client.gui.hud.isHidden()) client.gui.hud.toggle();
        });
        settle(context, singleplayer);
        context.runOnClient(client -> {
            BlockPos floor = new BlockPos(1, floorY, 0);
            BlockPos wall = new BlockPos(0, floorY + 2, 5);
            var lightmap = client.gameRenderer.gameRenderState().lightmapRenderState;
            System.out.println("LIGHTBOUND DIMENSION CLIENT " + label + " player=" + client.player.position()
                    + " camera=" + client.gameRenderer.gameRenderState().levelRenderState.cameraRenderState.pos
                    + " floor=" + client.level.getBlockState(floor) + " wall=" + client.level.getBlockState(wall)
                    + " ceiling=" + client.level.getBlockState(new BlockPos(0, floorY + 8, 0))
                    + " block=" + client.level.getBrightness(LightLayer.BLOCK, floor)
                    + " sky=" + client.level.getBrightness(LightLayer.SKY, floor)
                    + " belowSky=" + client.level.getBrightness(LightLayer.SKY, floor.below())
                    + " effectiveFloor=" + LightRules.getEffectiveLight(client.level, floor)
                    + " effectiveWall=" + LightRules.getEffectiveLight(client.level, wall)
                    + " skyDarken=" + client.level.getSkyDarken() + " skyFactor=" + lightmap.skyFactor
                    + " held=" + LightRules.heldLuminance(client.player));
        });
        server.runOnServer(ms -> {
            var player = connection.getServerPlayer();
            BlockPos floor = new BlockPos(1, floorY, 0);
            System.out.println("LIGHTBOUND DIMENSION SERVER " + label + " player=" + player.position()
                    + " dimension=" + player.level().dimension() + " floor=" + player.level().getBlockState(floor)
                    + " effectiveFloor=" + LightRules.getEffectiveLight(player.level(), floor)
                    + " held=" + LightRules.heldLuminance(player));
        });
        assertBlack(screenshot(context, "lightbound-" + label + "-arrival-dark"));

        BlockPos ground = new BlockPos(1, floorY, 0);
        assertServerCollision(singleplayer, ground, false, label);
        server.runOnServer(ms -> connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH)));
        connection.waitForClientboundPackets();
        context.waitFor(client -> BuiltinDynamicLighting.sourceCount() == 1);
        settle(context, singleplayer);
        context.runOnClient(client -> {
            if (!LightMath.isSolid(LightRules.getEffectiveLight(client.level, ground), THRESHOLD)
                    || BuiltinDynamicLighting.sampleLight(ground) < THRESHOLD) {
                throw new AssertionError(label + " held torch did not illuminate and support the ground");
            }
            if (client.level.getBrightness(LightLayer.BLOCK, ground) != 0) {
                throw new AssertionError(label + " dynamic light mutated the vanilla light engine");
            }
        });
        Path lit = screenshot(context, "lightbound-" + label + "-held-torch");
        if (pixels(lit).meanPeak < 40) throw new AssertionError(label + " held torch did not visibly illuminate native terrain: " + lit);
        assertServerCollision(singleplayer, ground, true, label);

        clearHeldLight(singleplayer);
        context.waitFor(client -> BuiltinDynamicLighting.sourceCount() == 0);
        settle(context, singleplayer);
        context.runOnClient(client -> {
            if (LightRules.getEffectiveLight(client.level, ground) >= THRESHOLD) {
                throw new AssertionError(label + " empty sealed room incorrectly supplies physical light");
            }
        });
        assertBlack(screenshot(context, "lightbound-" + label + "-pitch-dark"));
        assertServerCollision(singleplayer, ground, false, label);

        context.runOnClient(client -> ClientVisualSettings.gatewayVoid = true);
        context.waitTicks(8);
        Path gateway = screenshot(context, "lightbound-" + label + "-gateway-void");
        PixelStats cosmos = pixels(gateway);
        if (cosmos.meanPeak < 4 || cosmos.coloredFraction < 0.05) {
            throw new AssertionError(label + " gateway option did not render a colorful void: " + gateway + " " + cosmos);
        }
        context.runOnClient(client -> ClientVisualSettings.gatewayVoid = false);
        context.waitTicks(8);
        assertBlack(screenshot(context, "lightbound-" + label + "-gateway-disabled"));
    }

    private static void checkPlacedLava(ClientGameTestContext context, TestSingleplayerContext singleplayer) {
        var server = singleplayer.getServer();
        // Enclose the source so the assertion cannot leave flowing lava behind.
        server.runCommand("execute in minecraft:the_nether run fill 2 160 -4 4 162 -2 minecraft:glass hollow");
        server.runCommand("execute in minecraft:the_nether run setblock 3 161 -3 minecraft:lava");
        settle(context, singleplayer);
        BlockPos litFloor = new BlockPos(1, 160, -3);
        context.runOnClient(client -> {
            if (BuiltinDynamicLighting.sourceCount() != 0 || LightRules.getEffectiveLight(client.level, litFloor) < THRESHOLD) {
                throw new AssertionError("Placed Nether lava must support nearby ground without a held-light source");
            }
        });
        server.runCommand("execute in minecraft:the_nether run setblock 3 161 -3 minecraft:air");
        settle(context, singleplayer);
        context.runOnClient(client -> {
            if (LightRules.getEffectiveLight(client.level, litFloor) >= THRESHOLD) {
                throw new AssertionError("Removing placed Nether lava left stale physical light");
            }
        });
    }

    private static void clearHeldLight(TestSingleplayerContext singleplayer) {
        singleplayer.getServer().runOnServer(ms -> {
            var player = singleplayer.getConnection().getServerPlayer();
            player.setItemSlot(EquipmentSlot.MAINHAND, ItemStack.EMPTY);
            player.setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY);
        });
    }

    private static void assertServerCollision(TestSingleplayerContext singleplayer, BlockPos ground, boolean solid, String label) {
        singleplayer.getServer().runOnServer(ms -> {
            var player = singleplayer.getConnection().getServerPlayer();
            boolean flying = player.getAbilities().flying;
            try {
                // Query the real authoritative collision path without allowing a movement
                // tick or sending temporary flight settings to the client.
                player.getAbilities().flying = false;
                var level = player.level();
                boolean empty = level.getBlockState(ground).getCollisionShape(level, ground, CollisionContext.of(player)).isEmpty();
                if (empty == solid) {
                    throw new AssertionError(label + " server collision must be " + (solid ? "solid with the held torch" : "empty without light"));
                }
            } finally {
                player.getAbilities().flying = flying;
            }
        });
    }

    private static void settle(ClientGameTestContext context, TestSingleplayerContext singleplayer) {
        context.waitTicks(20);
        singleplayer.getConnection().waitForClientboundPackets();
        singleplayer.getConnection().waitForChunksRender();
    }

    private static Path screenshot(ClientGameTestContext context, String name) {
        return context.takeScreenshot(TestScreenshotOptions.of(name).withSize(1280, 720).disableCounterPrefix());
    }

    private static void assertBlack(Path image) {
        PixelStats stats = pixels(image);
        if (stats.maximum > 2) throw new AssertionError("Unlit dimension terrain is not black: " + image + " " + stats);
    }

    private static PixelStats pixels(Path path) {
        try {
            BufferedImage image = ImageIO.read(path.toFile());
            if (image == null) throw new AssertionError("Unreadable screenshot " + path);
            long total = 0;
            int count = 0;
            int colored = 0;
            int maximum = 0;
            for (int y = image.getHeight() / 3; y < image.getHeight() * 2 / 3; y++) {
                for (int x = image.getWidth() / 3; x < image.getWidth() * 2 / 3; x++) {
                    int rgb = image.getRGB(x, y);
                    int r = (rgb >>> 16) & 255;
                    int g = (rgb >>> 8) & 255;
                    int b = rgb & 255;
                    int peak = Math.max(r, Math.max(g, b));
                    total += peak;
                    maximum = Math.max(maximum, peak);
                    if (peak - Math.min(r, Math.min(g, b)) >= 3) ++colored;
                    ++count;
                }
            }
            return new PixelStats(total / (double) count, maximum, colored / (double) count);
        } catch (IOException exception) {
            throw new AssertionError("Could not inspect " + path, exception);
        }
    }

    private record PixelStats(double meanPeak, int maximum, double coloredFraction) {}

    private record Settings(boolean dynamic, boolean binary, boolean gateway, boolean edge, boolean particles,
                            double brightness, double glow, double density, boolean hudHidden) {
        static Settings capture(boolean hidden) {
            return new Settings(ClientVisualSettings.dynamicLighting, ClientVisualSettings.binaryLighting,
                    ClientVisualSettings.gatewayVoid, ClientVisualSettings.edgeGlow, ClientVisualSettings.auraParticles,
                    ClientVisualSettings.litBrightness, ClientVisualSettings.glowStrength, ClientVisualSettings.particleDensity, hidden);
        }

        void restore() {
            ClientVisualSettings.dynamicLighting = dynamic;
            ClientVisualSettings.binaryLighting = binary;
            ClientVisualSettings.gatewayVoid = gateway;
            ClientVisualSettings.edgeGlow = edge;
            ClientVisualSettings.auraParticles = particles;
            ClientVisualSettings.litBrightness = brightness;
            ClientVisualSettings.glowStrength = glow;
            ClientVisualSettings.particleDensity = density;
            ClientVisualSettings.revision++;
        }
    }
}
