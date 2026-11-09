package dev.lightbound;

import java.awt.image.BufferedImage;
import java.io.IOException;
import java.nio.file.Path;
import javax.imageio.ImageIO;

import net.fabricmc.fabric.api.client.gametest.v1.FabricClientGameTest;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.fabricmc.fabric.api.client.gametest.v1.screenshot.TestScreenshotOptions;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;

/** Exercises the actual lightmap, dynamic renderer, and synchronized collision inputs. */
@SuppressWarnings("UnstableApiUsage")
public class LightboundClientGameTest implements FabricClientGameTest {
    @Override
    public void runTest(ClientGameTestContext context) {
        context.runOnClient(client -> {
            dev.lightbound.client.ClientVisualSettings.resetDefaults();
            AuraParticleChecks.run();
        });
        try (TestSingleplayerContext singleplayer = context.worldBuilder().create()) {
            var server = singleplayer.getServer();
            var connection = singleplayer.getConnection();
            server.runCommand("gamerule lightbound:afterglow_ticks 0");
            server.runCommand("gamemode creative @p");
            server.runCommand("fill -8 80 -8 8 89 8 minecraft:stone hollow");
            server.runCommand("fill -7 80 -7 7 80 7 minecraft:smooth_stone");
            server.runCommand("fill -7 81 8 7 88 8 minecraft:white_concrete");
            server.runCommand("fill -3 81 5 -3 85 5 minecraft:stone_bricks");
            server.runCommand("fill 3 81 5 3 85 5 minecraft:stone_bricks");
            server.runCommand("setblock 0 80 0 minecraft:bedrock");
            server.runCommand("tp @p 0.5 81 0.5 0 15");
            server.runOnServer(ms -> { var p = connection.getServerPlayer(); p.getAbilities().flying = true; p.onUpdateAbilities(); });
            server.runCommand("time set midnight");
            server.runCommand("weather clear");
            context.runOnClient(client -> {
                client.options.gamma().set(1.0);
                if (!client.gui.hud.isHidden()) {
                    client.gui.hud.toggle();
                }
            });
            context.waitTicks(30);
            connection.waitForClientboundPackets();
            connection.waitForChunksRender();

            server.runOnServer(minecraftServer -> connection.getServerPlayer()
                    .setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH)));
            context.waitTicks(30);
            connection.waitForClientboundPackets();
            connection.waitForChunksRender();
            double heldLight = context.computeOnClient(client ->
                    LightRules.getEffectiveLight(client.level, new BlockPos(1, 80, 0)));
            if (heldLight < 1.0) {
                throw new AssertionError("A held torch must make nearby ground solid: " + heldLight);
            }
            Path torch = screenshot(context, "lightbound-held-torch");
            assertVisible(torch, "The held torch must illuminate actual world geometry");
            dev.lightbound.client.ClientMenuChecks.run(context);
            dev.lightbound.client.ClientGameplayMenuChecks.run(context, singleplayer);
            ClientLightingChecks.run(context, singleplayer);

            server.runCommand("gamerule lightbound:minimum_light 8");
            connection.waitForClientboundPackets();
            context.waitFor(client -> LightboundSettings.threshold(client.level) == 8);
            server.runCommand("gamerule lightbound:minimum_light 1");
            connection.waitForClientboundPackets();
            context.waitFor(client -> LightboundSettings.threshold(client.level) == 1);

            server.runCommand("tp @p 0.5 81 0.5 0 35");
            context.runOnClient(client -> {
                if (client.gui.hud.isHidden()) {
                    client.gui.hud.toggle();
                }
            });
            context.waitTicks(10);
            connection.waitForClientboundPackets();
            connection.waitForChunksRender();
            Path outline = screenshot(context, "lightbound-torch-outline");
            assertAmberOutline(outline);
            context.runOnClient(c -> {
                dev.lightbound.client.ClientVisualSettings.gatewayVoid = true;
                dev.lightbound.client.ClientVisualSettings.glowColor = 0x24DFFF;
                dev.lightbound.client.ClientVisualSettings.changed();
            });
            context.waitTicks(10);
            context.takeScreenshot(TestScreenshotOptions.of("lightbound-rgb-rainbow-preview").disableCounterPrefix());
            context.runOnClient(c -> dev.lightbound.client.ClientVisualSettings.resetDefaults());

            server.runCommand("tp @p 0.5 81 0.5 0 15");
            context.runOnClient(client -> {
                if (!client.gui.hud.isHidden()) {
                    client.gui.hud.toggle();
                }
            });

            server.runOnServer(minecraftServer -> connection.getServerPlayer()
                    .setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY));
            context.waitTicks(30);
            connection.waitForClientboundPackets();
            connection.waitForChunksRender();
            Path dark = screenshot(context, "lightbound-pitch-black-max-brightness");
            assertBlack(dark);
            GatewayVoidChecks.run(context);

            server.runCommand("effect give @p minecraft:night_vision 30 0 true");
            context.waitTicks(20);
            connection.waitForClientboundPackets();
            Path nightVision = screenshot(context, "lightbound-night-vision");
            assertVisible(nightVision, "Night vision should improve sight without creating physical light");
            double potionLight = context.computeOnClient(client ->
                    LightRules.getEffectiveLight(client.level, new BlockPos(1, 80, 0)));
            if (potionLight >= 1.0) {
                throw new AssertionError("Night vision must not solidify dark ground: " + potionLight);
            }

            server.runCommand("effect clear @p");
            server.runCommand("fill -8 89 -8 8 89 8 minecraft:air");
            server.runCommand("time set noon");
            context.waitTicks(40);
            connection.waitForClientboundPackets();
            connection.waitForChunksRender();
            Path daylight = screenshot(context, "lightbound-daylight");
            assertVisible(daylight, "Ordinary daylight must still illuminate the world");

            // A fixed, placed-light pit: its rim must not be a held-torch radius.
            server.runOnServer(ms -> connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY));
            server.runCommand("fill 20 100 -20 60 100 20 minecraft:smooth_stone");
            server.runCommand("fill 20 99 -20 60 99 20 minecraft:glowstone");
            server.runCommand("fill 39 99 1 41 99 4 minecraft:air");
            server.runCommand("fill 39 100 1 41 100 4 minecraft:air");
            server.runCommand("gamerule lightbound:minimum_light 15");
            server.runCommand("tp @p 40.5 101 -3.5 0 30");
            server.runCommand("time set midnight");
            context.runOnClient(client -> { if (client.gui.hud.isHidden()) client.gui.hud.toggle(); });
            context.waitTicks(40);
            connection.waitForClientboundPackets();
            connection.waitForChunksRender();
            var pitEdges = context.computeOnClient(client -> VoidEdgeScanner.scan(client.level,
                    client.player.blockPosition(), client.player.getY(), client.player.getBbHeight(), 15));
            if (pitEdges.size() != 14) throw new AssertionError("Expected only the fixed 3x4 pit rim, got " + pitEdges);
            context.takeScreenshot(TestScreenshotOptions.of("lightbound-void-rim-glow").disableCounterPrefix());
            context.runOnClient(c -> {
                dev.lightbound.client.ClientVisualSettings.gatewayVoid = true;
                dev.lightbound.client.ClientVisualSettings.changed();
            });
            context.waitTicks(10);
            context.takeScreenshot(TestScreenshotOptions.of("lightbound-rainbow-aura-preview").disableCounterPrefix());
            context.runOnClient(c -> {
                dev.lightbound.client.ClientVisualSettings.gatewayVoid = false;
                dev.lightbound.client.ClientVisualSettings.glowColor = 0x24DFFF;
                dev.lightbound.client.ClientVisualSettings.changed();
            });
            context.waitTicks(5);
            var cyanGlow = GatewayVoidChecks.capture(context, "lightbound-cyan-glow");
            int cyanPixels = 0;
            for (int y = 0; y < cyanGlow.getHeight() * 3 / 4; y++) {
                for (int x = cyanGlow.getWidth() / 4; x < cyanGlow.getWidth() * 3 / 4; x++) {
                    int rgb = cyanGlow.getRGB(x, y);
                    if (((rgb >> 16) & 255) < 80 && ((rgb >> 8) & 255) > 120 && (rgb & 255) > 140) cyanPixels++;
                }
            }
            if (cyanPixels < 30) throw new AssertionError("RGB glow did not update live: " + cyanPixels);
            context.runOnClient(c -> dev.lightbound.client.ClientVisualSettings.resetDefaults());

            // View the same broad hillside in daylight and with a single torch.
            server.runCommand("fill 60 118 -24 100 118 18 minecraft:stone");
            server.runCommand("fill 60 119 0 100 119 18 minecraft:grass_block");
            server.runCommand("fill 60 120 5 100 120 18 minecraft:grass_block");
            server.runCommand("fill 60 121 9 100 121 18 minecraft:grass_block");
            server.runCommand("gamerule lightbound:minimum_light 1");
            server.runCommand("time set noon");
            server.runCommand("tp @p 80.5 124 -1.5 0 49");
            context.waitTicks(40);
            connection.waitForClientboundPackets();
            connection.waitForChunksRender();
            context.runOnClient(c -> {
                var edges = VoidEdgeScanner.scan(c.level, c.player.blockPosition(), c.player.getY(), c.player.getBbHeight(), 1);
                if (!edges.isEmpty()) throw new AssertionError("Sunlit hillside must have zero lines: " + edges);
            });
            if (amberPixels(screenshot(context, "lightbound-sunlit-steps")) != 0)
                throw new AssertionError("Sunlit terraces still show gold in the rendered world");

            server.runCommand("time set midnight");
            server.runCommand("setblock 80 120 3 minecraft:torch");
            server.runCommand("gamerule lightbound:minimum_light 9");
            server.runOnServer(ms -> connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY));
            context.waitTicks(30);
            connection.waitForClientboundPackets();
            connection.waitForChunksRender();
            context.runOnClient(c -> {
                int threshold = LightboundSettings.threshold(c.level);
                if (threshold != 9 || LightRules.getEffectiveLight(c.level, c.player.blockPosition()) >= threshold)
                    throw new AssertionError("Contour fixture must put the observer outside the light");
                var edges = VoidEdgeScanner.scan(c.level, c.player.blockPosition(), c.player.getY(), c.player.getBbHeight(), threshold);
                if (edges.isEmpty()) throw new AssertionError("The torch perimeter must be visible from unlit air");
                int darkLandings = 0, litLandings = 0;
                for (int x = 74; x <= 86; x++) {
                    if (LightRules.getEffectiveLight(c.level, new BlockPos(x, 119, 0)) < threshold) continue;
                    boolean voidBelow = LightRules.getEffectiveLight(c.level, new BlockPos(x, 118, -1)) < threshold;
                    boolean rim = edges.contains(new VoidEdgeScanner.Edge(x, 120.008, 0, x + 1, 120.008, 0));
                    if (rim != voidBelow) throw new AssertionError("Incorrect rim across the step at x=" + x);
                    if (voidBelow) darkLandings++; else litLandings++;
                }
                if (litLandings == 0 || darkLandings == 0)
                    throw new AssertionError("The fixture must include both lit and phased lower steps");
            });
            assertAmberOutline(screenshot(context, "lightbound-terraces-from-darkness"));
            context.runOnClient(c -> { dev.lightbound.client.ClientVisualSettings.curvedEdges = true; dev.lightbound.client.ClientVisualSettings.changed(); });
            context.waitTicks(3);
            context.takeScreenshot(TestScreenshotOptions.of("lightbound-curved-terraces").disableCounterPrefix());
            context.runOnClient(c -> dev.lightbound.client.ClientVisualSettings.resetDefaults());
            server.runCommand("gamerule lightbound:minimum_light 1");

            ClientLightingChecks.lichenOnMountain(context, singleplayer);
            // Exercise prediction and server movement validation, not only isolated shapes.
            server.runCommand("fill -1 81 -1 2 83 1 minecraft:stone");
            server.runCommand("gamemode survival @p");
            server.runOnServer(ms -> {
                var p = connection.getServerPlayer();
                p.setNoGravity(true);
                p.setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH));
            });
            server.runCommand("tp @p 0.5 81.1 0.5");
            connection.waitForClientboundPackets();
            context.waitTicks(5);
            context.runOnClient(c -> { if (!c.gui.hud.isHidden()) c.gui.hud.toggle(); });
            assertBlack(screenshot(context, "lightbound-no-xray-inside-terrain"));
            context.runOnClient(c -> { if (c.gui.hud.isHidden()) c.gui.hud.toggle(); });
            for (int tick = 0; tick < 20; tick++) {
                context.runOnClient(client -> { client.player.setNoGravity(true); client.player.setDeltaMovement(0.2, 0, 0); });
                context.waitTicks(1);
                context.runOnClient(c -> System.out.println("ESCAPE " + c.player.position()+" escaping="+LightRules.isEscapingTerrain(c.player)+" flying="+c.player.getAbilities().flying));
            }
            context.runOnClient(client -> client.player.setDeltaMovement(0, 0, 0));
            context.waitTicks(5);
            double clientX = context.computeOnClient(client -> client.player.getX());
            double serverX = server.computeOnServer(ms -> connection.getServerPlayer().getX());
            if (clientX < 3.3 || Math.abs(clientX - serverX) > 0.5) {
                throw new AssertionError("Embedded player snagged or server corrected movement: client="+clientX+", server="+serverX);
            }

        }
    }

    private static Path screenshot(ClientGameTestContext context, String name) {
        return context.takeScreenshot(TestScreenshotOptions.of(name)
                .withSize(1280, 720).disableCounterPrefix());
    }

    private static BufferedImage read(Path path) {
        try {
            BufferedImage image = ImageIO.read(path.toFile());
            if (image == null) {
                throw new AssertionError("Unreadable screenshot: " + path);
            }
            return image;
        } catch (IOException exception) {
            throw new AssertionError("Could not inspect screenshot: " + path, exception);
        }
    }

    private static void assertBlack(Path path) {
        BufferedImage image = read(path);
        // The central room contains only nonemissive blocks, no sky, HUD, or particles.
        // Allow two channel values for driver/color conversion rounding.
        for (int y = image.getHeight() / 3; y < image.getHeight() * 2 / 3; y++) {
            for (int x = image.getWidth() / 4; x < image.getWidth() * 3 / 4; x++) {
                int rgb = image.getRGB(x, y);
                if (((rgb >>> 16) & 255) > 2 || ((rgb >>> 8) & 255) > 2 || (rgb & 255) > 2) {
                    throw new AssertionError("Unlit terrain is not black at maximum brightness: "
                            + path + " pixel " + x + "," + y + " = " + Integer.toHexString(rgb));
                }
            }
        }
    }

    private static void assertVisible(Path path, String message) {
        BufferedImage image = read(path);
        long total = 0;
        int count = 0;
        for (int y = image.getHeight() / 3; y < image.getHeight() * 2 / 3; y++) {
            for (int x = image.getWidth() / 4; x < image.getWidth() * 3 / 4; x++) {
                int rgb = image.getRGB(x, y);
                total += Math.max((rgb >>> 16) & 255, Math.max((rgb >>> 8) & 255, rgb & 255));
                count++;
            }
        }
        if (total / (double) count < 5.0) {
            throw new AssertionError(message + ": " + path);
        }
    }

    private static void assertAmberOutline(Path path) {
        int count = amberPixels(path);
        if (count < 20) throw new AssertionError("Expected a visible amber safe-ground perimeter, found "
                + count + " matching pixels: " + path);
    }

    private static int amberPixels(Path path) {
        BufferedImage image = read(path);
        int amberPixels = 0;
        // Exclude the torch in the lower-left hand and the hotbar. Look for the
        // renderer's actual amber color on the central world-space perimeter.
        for (int y = image.getHeight() / 10; y < image.getHeight() * 13 / 20; y++) {
            for (int x = image.getWidth() / 4; x < image.getWidth() * 3 / 4; x++) {
                int rgb = image.getRGB(x, y);
                int red = (rgb >>> 16) & 255;
                int green = (rgb >>> 8) & 255;
                int blue = rgb & 255;
                if (red >= 235 && Math.abs(green - 181) <= 20 && Math.abs(blue - 46) <= 20) {
                    amberPixels++;
                }
            }
        }
        return amberPixels;
    }
}
