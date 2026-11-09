package dev.lightbound;

import dev.lightbound.client.ClientVisualSettings;
import java.awt.Color;
import java.awt.image.BufferedImage;
import java.nio.file.Path;
import java.util.HashSet;
import javax.imageio.ImageIO;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.screenshot.TestScreenshotOptions;
import net.minecraft.core.BlockPos;

@SuppressWarnings("UnstableApiUsage")
public final class GatewayVoidChecks {
    private GatewayVoidChecks() {}

    /** Called in the enclosed, unlit room. Rendering the experiment cannot restore support. */
    public static void run(ClientGameTestContext context) {
        context.runOnClient(c -> { ClientVisualSettings.gatewayVoid = true; ClientVisualSettings.changed(); });
        context.waitTicks(5);
        BufferedImage first = capture(context, "lightbound-rainbow-void");
        var hues = new HashSet<Integer>();
        double brightness = 0;
        int pixels = 0;
        for (int y = first.getHeight() / 4; y < first.getHeight() * 3 / 4; y += 3) {
            for (int x = first.getWidth() / 4; x < first.getWidth() * 3 / 4; x += 3) {
                int color = first.getRGB(x, y);
                float[] hsv = Color.RGBtoHSB((color >> 16) & 255, (color >> 8) & 255, color & 255, null);
                brightness += hsv[2] * 255;
                pixels++;
                if (hsv[1] > 0.3 && hsv[2] > 0.02) hues.add((int)(hsv[0] * 12));
            }
        }
        if (brightness / pixels < 4 || hues.size() < 3) {
            throw new AssertionError("Rainbow void shader missing or spatially uniform: brightness=" + brightness / pixels + ", hues=" + hues);
        }
        context.waitTicks(20);
        BufferedImage second = capture(context, "lightbound-rainbow-void-one-second");
        double change = difference(first, second);
        if (change > 3 || change < 0.0001) throw new AssertionError("Void should drift very slowly: one-second pixel change=" + change);
        double light = context.computeOnClient(c -> LightRules.getEffectiveLight(c.level, new BlockPos(1, 80, 0)));
        if (light >= 1) throw new AssertionError("Cosmetic rainbow void created physical light: " + light);
        context.runOnClient(c -> { ClientVisualSettings.gatewayVoid = false; ClientVisualSettings.changed(); });
        context.waitTicks(5);
        BufferedImage disabled = capture(context, "lightbound-rainbow-void-disabled");
        int center = disabled.getRGB(disabled.getWidth() / 2, disabled.getHeight() / 2) & 0xFFFFFF;
        if (center != 0) throw new AssertionError("Disabling rainbow void did not restore pitch darkness: " + center);
        System.out.println("LIGHTBOUND VOID VERIFIED hues=" + hues.size() + " brightness=" + brightness / pixels + " oneSecondChange=" + change);
    }

    public static BufferedImage capture(ClientGameTestContext context, String name) {
        Path path = context.takeScreenshot(TestScreenshotOptions.of(name).disableCounterPrefix());
        try { return ImageIO.read(path.toFile()); }
        catch (Exception e) { throw new AssertionError("Cannot read " + path, e); }
    }

    private static double difference(BufferedImage a, BufferedImage b) {
        long sum = 0;
        int count = 0;
        for (int y = a.getHeight() / 4; y < a.getHeight() * 3 / 4; y += 3) {
            for (int x = a.getWidth() / 4; x < a.getWidth() * 3 / 4; x += 3) {
                int p = a.getRGB(x, y), q = b.getRGB(x, y);
                for (int shift : new int[]{0, 8, 16}) { sum += Math.abs(((p >> shift) & 255) - ((q >> shift) & 255)); count++; }
            }
        }
        return sum / (double) count;
    }
}
