package dev.lightbound;

import dev.lightbound.client.ClientVisualSettings;
import java.awt.image.BufferedImage;
import java.nio.file.Path;
import javax.imageio.ImageIO;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.fabricmc.fabric.api.client.gametest.v1.screenshot.TestScreenshotOptions;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;

/** Compare the same white surface under two different, physically valid light levels. */
@SuppressWarnings("UnstableApiUsage")
public final class ClientLightingChecks {
    private ClientLightingChecks() {}

    public static void run(ClientGameTestContext context, TestSingleplayerContext singleplayer) {
        var server = singleplayer.getServer();
        var connection = singleplayer.getConnection();
        server.runCommand("fill -4 81 3 4 86 3 minecraft:white_concrete");
        server.runCommand("tp @p 0.5 81 0.5 0 0");
        settle(context, singleplayer);
        double bright = centerBrightness(capture(context, "lightbound-uniform-torch"));
        context.runOnClient(c -> { ClientVisualSettings.gatewayVoid = true; ClientVisualSettings.changed(); });
        context.waitTicks(5);
        double portalLit = centerBrightness(capture(context, "lightbound-rainbow-preserves-lit-terrain"));
        if (Math.abs(bright - portalLit) > 2) throw new AssertionError("Rainbow void altered visible terrain: " + bright + " vs " + portalLit);
        context.runOnClient(c -> { ClientVisualSettings.gatewayVoid = false; ClientVisualSettings.changed(); });

        server.runOnServer(ms -> connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND,
                new ItemStack(Items.REDSTONE_TORCH)));
        settle(context, singleplayer);
        double physicalLight = context.computeOnClient(c -> LightRules.getEffectiveLight(c.level, new BlockPos(0, 82, 3)));
        if (physicalLight < 1 || physicalLight > 3) {
            throw new AssertionError("Weak held torch fixture must have dim but valid support: " + physicalLight);
        }
        double dim = centerBrightness(capture(context, "lightbound-uniform-weak-light"));
        if (bright < 80 || Math.abs(bright - dim) > 3) {
            throw new AssertionError("Uniform mode changed brightness with attenuation: strong=" + bright + ", weak=" + dim);
        }

        // Aim at the centre of the adjacent white block whose physical light is below 1.
        // This catches interpolated vertex/lightmap fringes that would falsely look safe.
        server.runCommand("tp @p 0.5 81 0.5 -21.8014 2.552");
        settle(context, singleplayer);
        double fringeLight = context.computeOnClient(c -> LightRules.getEffectiveLight(c.level, new BlockPos(1, 82, 3)));
        double fringe = centerBrightness(capture(context, "lightbound-binary-unsafe-fringe"));
        if (fringeLight >= 1 || fringe > 2) throw new AssertionError("Unsafe block has visible gradient: light=" + fringeLight + ", pixel=" + fringe);
        server.runCommand("tp @p 0.5 81 0.5 0 0");
        server.runCommand("gamerule lightbound:minimum_light 2");
        settle(context, singleplayer);
        double thresholdDark = centerBrightness(capture(context, "lightbound-binary-raised-threshold"));
        if (thresholdDark > 2) throw new AssertionError("Changing server threshold left visibly lit unsafe terrain: " + thresholdDark);
        server.runCommand("gamerule lightbound:minimum_light 1");
        settle(context, singleplayer);

        context.runOnClient(c -> { ClientVisualSettings.binaryLighting = false; ClientVisualSettings.changed(); });
        settle(context, singleplayer);
        double gradient = centerBrightness(capture(context, "lightbound-gradient-weak-light"));
        if (gradient >= dim * 0.6) throw new AssertionError("Gradient toggle did not restore falloff: " + gradient + " vs " + dim);

        context.runOnClient(c -> {
            ClientVisualSettings.binaryLighting = true;
            ClientVisualSettings.dynamicLighting = false;
            ClientVisualSettings.changed();
        });
        settle(context, singleplayer);
        double disabled = centerBrightness(capture(context, "lightbound-dynamic-light-disabled"));
        double unchanged = context.computeOnClient(c -> LightRules.getEffectiveLight(c.level, new BlockPos(0, 82, 3)));
        if (disabled > 2 || Math.abs(physicalLight - unchanged) > 0.001) {
            throw new AssertionError("Visual lighting switch changed support or left stale light: pixel=" + disabled + ", light=" + unchanged);
        }
        context.runOnClient(c -> { ClientVisualSettings.dynamicLighting = true; ClientVisualSettings.changed(); });
        boolean intermediate = false;
        for (int frame = 0; frame < 8; frame++) {
            context.waitTick();
            double sample = centerBrightness(capture(context, "lightbound-fade-" + frame));
            System.out.println("FADE SAMPLE " + frame + "=" + sample);
            intermediate |= sample > 3 && sample < dim - 3;
        }
        if (!intermediate) throw new AssertionError("Lighting must pass through visible intermediate brightness during its fade");
        context.runOnClient(c -> { ClientVisualSettings.dynamicLighting = false; ClientVisualSettings.changed(); });
        boolean fadingOut = false;
        for (int frame = 0; frame < 8; frame++) {
            context.waitTick();
            double sample = centerBrightness(capture(context, "lightbound-fade-out-" + frame));
            System.out.println("FADE OUT SAMPLE " + frame + "=" + sample);
            fadingOut |= sample > 3 && sample < dim - 3;
        }
        if (!fadingOut) throw new AssertionError("Removing light must visibly fade out");
        context.runOnClient(c -> { ClientVisualSettings.dynamicLighting = true; ClientVisualSettings.changed(); });


        settle(context, singleplayer);
        double restored = centerBrightness(capture(context, "lightbound-dynamic-light-restored"));
        if (Math.abs(restored - dim) > 3) throw new AssertionError("Reenabling dynamic lighting failed: " + restored);

        // Brightness is live and visual only, including changes while terrain is stationary.
        context.runOnClient(c -> { ClientVisualSettings.litBrightness = 0.4; ClientVisualSettings.changed(); });
        context.waitTicks(5);
        double lowered = centerBrightness(capture(context, "lightbound-uniform-brightness-slider"));
        if (lowered >= restored * 0.8 || lowered < 10) throw new AssertionError("Live brightness did not update: " + lowered);

        context.runOnClient(c -> ClientVisualSettings.resetDefaults());
        server.runOnServer(ms -> connection.getServerPlayer().setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH)));
        server.runCommand("fill -4 81 3 4 86 3 minecraft:air");
        server.runCommand("tp @p 0.5 81 0.5 0 15");
        settle(context, singleplayer);
        System.out.println("LIGHTBOUND LIGHTING VERIFIED uniform=" + bright + "/" + dim + " gradient=" + gradient
                + " disabled=" + disabled + " restored=" + restored + " slider=" + lowered);
    }

    static void lichenOnMountain(ClientGameTestContext context, TestSingleplayerContext singleplayer) {
        var server = singleplayer.getServer();
        boolean hidden = context.computeOnClient(c -> c.gui.hud.isHidden());
        context.runOnClient(c -> { if (!hidden) c.gui.hud.toggle(); });
        try {
            server.runCommand("time set midnight");
            server.runCommand("tp @p 80.5 184 24.5 0 0");
            settle(context, singleplayer);
            server.runCommand("fill 67 172 32 93 199 32 minecraft:white_concrete");
            server.runCommand("setblock 80 185 31 minecraft:glow_lichen[south=true]");
            settle(context, singleplayer);
            context.runOnClient(c -> {
                var lichen = new BlockPos(80, 185, 31);
                if (!c.level.getBlockState(lichen).is(net.minecraft.world.level.block.Blocks.GLOW_LICHEN)
                        || LightRules.getEffectiveLight(c.level, lichen.south()) < 7)
                    throw new AssertionError("Fixture must have physical light from wall lichen");
            });
            double near = centerBrightness(capture(context, "uncommon-nights-lichen-near"));
            server.runCommand("tp @p 80.5 184 14.5 0 0");
            settle(context, singleplayer);
            double mid = centerBrightness(capture(context, "uncommon-nights-lichen-fading"));
            server.runCommand("tp @p 80.5 184 0.5 0 0");
            settle(context, singleplayer);
            Path distant = capture(context, "uncommon-nights-lichen-distant");
            assertDarkMountain(distant);
            double far = centerBrightness(distant);
            if (near < 40 || mid <= far + 3 || mid >= near - 3 || far > 2)
                throw new AssertionError("Expected nearby lichen and a black distant wall: " + near + "/" + mid + "/" + far);

            // A weaker overlapping source still lights the SAME distant patch.
            server.runCommand("setblock 84 185 31 minecraft:redstone_wall_torch[facing=north]");
            settle(context, singleplayer);
            double torch = centerBrightness(capture(context, "uncommon-nights-lichen-with-torch"));
            if (torch < 40) throw new AssertionError("Lichen swallowed a nearby redstone torch: " + torch);
            server.runCommand("setblock 84 185 31 minecraft:air");
            context.runOnClient(c -> { ClientVisualSettings.binaryLighting = false; ClientVisualSettings.changed(); });
            settle(context, singleplayer);
            Path gradientImage = capture(context, "uncommon-nights-lichen-gradient-distant");
            assertDarkMountain(gradientImage);
            double gradient = centerBrightness(gradientImage);
            if (gradient > 2) throw new AssertionError("Gradient mode left a distant lichen patch: " + gradient);
            server.runCommand("time set noon");
            settle(context, singleplayer);
            if (centerBrightness(capture(context, "uncommon-nights-lichen-daylight")) < 30)
                throw new AssertionError("Daylight must illuminate the same mountain");
            System.out.println("LICHEN VERIFIED near/mid/far=" + near + "/" + mid + "/" + far
                    + " overlappingTorch=" + torch + " gradientFar=" + gradient);
        } finally {
            server.runCommand("time set midnight");
            context.runOnClient(c -> {
                ClientVisualSettings.binaryLighting = true;
                ClientVisualSettings.changed();
                if (!hidden && c.gui.hud.isHidden()) c.gui.hud.toggle();
            });
        }
    }

    private static void assertDarkMountain(Path path) {
        try {
            BufferedImage image = ImageIO.read(path.toFile());
            // At 32 blocks this covers the lichen and its surrounding stone, across a section seam.
            for (int y = image.getHeight()*3/8; y < image.getHeight()*5/8; y++) {
                for (int x = image.getWidth()*2/5; x < image.getWidth()*3/5; x++) {
                    int rgb = image.getRGB(x, y);
                    if ((rgb & 255) > 2 || ((rgb >> 8) & 255) > 2 || ((rgb >> 16) & 255) > 2)
                        throw new AssertionError("Visible distant lichen-lit terrain at " + x + "," + y + " in " + path);
                }
            }
        } catch (java.io.IOException e) {
            throw new AssertionError("Cannot read mountain screenshot " + path, e);
        }
    }

    private static void settle(ClientGameTestContext context, TestSingleplayerContext singleplayer) {
        context.waitTicks(30);
        singleplayer.getConnection().waitForClientboundPackets();
        singleplayer.getConnection().waitForChunksRender();
    }

    private static Path capture(ClientGameTestContext context, String name) {
        return context.takeScreenshot(TestScreenshotOptions.of(name).disableCounterPrefix());
    }

    private static double centerBrightness(Path path) {
        try {
            BufferedImage image = ImageIO.read(path.toFile());
            long sum = 0;
            int count = 0;
            for (int y = image.getHeight() / 2 - 10; y < image.getHeight() / 2 + 10; y++) {
                for (int x = image.getWidth() / 2 - 10; x < image.getWidth() / 2 + 10; x++) {
                    int rgb = image.getRGB(x, y);
                    sum += Math.max((rgb >>> 16) & 255, Math.max((rgb >>> 8) & 255, rgb & 255));
                    count++;
                }
            }
            return sum / (double) count;
        } catch (Exception e) {
            throw new AssertionError("Could not read screenshot " + path, e);
        }
    }
}
