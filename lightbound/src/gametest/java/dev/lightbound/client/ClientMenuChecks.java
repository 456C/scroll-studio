package dev.lightbound.client;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.platform.InputConstants;
import dev.lightbound.LightboundSettings;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import javax.imageio.ImageIO;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.screenshot.TestScreenshotOptions;
import net.fabricmc.fabric.api.client.screen.v1.Screens;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.server.permissions.PermissionSet;

/** Invoked explicitly by the client integration test while a world is open. */
@SuppressWarnings("UnstableApiUsage")
public final class ClientMenuChecks {
    private ClientMenuChecks() {}

    public static void run(ClientGameTestContext context) {
        context.runOnClient(client -> verifyPersistence());
        Snapshot original = context.computeOnClient(client -> Snapshot.capture());
        int[] window = context.computeOnClient(client -> new int[]{client.getWindow().getWidth(),
                client.getWindow().getHeight(), client.options.guiScale().get()});
        int threshold = context.computeOnClient(client -> LightboundSettings.threshold(client.level));
        try {
            context.runOnClient(client -> {
                require(client.level != null, "Menu test requires a loaded world");
                ClientVisualSettings.resetDefaults();
                client.gui.setScreen(null);
                client.options.guiScale().set(1);
                client.resizeGui();
            });
            context.getInput().resizeWindow(320, 240);
            context.getInput().pressKey(InputConstants.KEY_K);
            context.waitForScreen(LightboundSettingsScreen.class);
            context.runOnClient(client -> checkLayout(client, 320, 240));
            context.takeScreenshot(TestScreenshotOptions.of("lightbound-menu-minimum").withSize(320, 240));
            long startTime = context.computeOnClient(client -> client.level.getGameTime());
            context.waitTicks(3);
            require(context.computeOnClient(client -> client.level.getGameTime()) > startTime,
                    "Settings must keep the world ticking for live preview");

            PermissionSet permissions = context.computeOnClient(client -> client.player.permissions());
            try {
                context.runOnClient(client -> client.player.setPermissions(PermissionSet.NO_PERMISSIONS));
                context.clickScreenButton("lightbound.settings.gameplay");
                context.waitForScreen(LightboundGameplayScreen.class);
                context.runOnClient(client -> {
                    checkLayout(client, 320, 240);
                    long editable = Screens.getWidgets(client.gui.screen()).stream().filter(widget -> widget.active).count();
                    require(editable > 1, "The host may edit settings even without operator permission");
                });
                context.takeScreenshot(TestScreenshotOptions.of("lightbound-gameplay-readonly-minimum").withSize(320, 240));
                context.clickScreenButton("gui.back");
                context.waitForScreen(LightboundSettingsScreen.class);
            } finally {
                context.runOnClient(client -> client.player.setPermissions(permissions));
            }

            context.clickScreenButton("lightbound.settings.glowColor.open");
            context.waitForScreen(LightboundGlowColorScreen.class);
            context.runOnClient(client -> checkLayout(client, 320, 240));
            click(context, "glowColor.red", 0.25);
            click(context, "glowColor.green", 0.5);
            click(context, "glowColor.blue", 0.75);
            int rgb = ClientVisualSettings.glowColor;
            require(((rgb >> 16) & 255) > 50 && ((rgb >> 16) & 255) < 80
                            && ((rgb >> 8) & 255) > 115 && ((rgb >> 8) & 255) < 140
                            && (rgb & 255) > 175 && (rgb & 255) < 205,
                    "All three RGB sliders must update their independent color channel live");
            Path colorScreenshot = context.takeScreenshot(TestScreenshotOptions.of("lightbound-glow-color-minimum")
                    .withSize(320, 240));
            checkSwatch(colorScreenshot, rgb);
            context.clickScreenButton("lightbound.settings.glowColor.reset");
            require(ClientVisualSettings.glowColor == 0xFFB52E, "Reset to gold must restore the original glow");
            click(context, "glowColor.red", 0.25);
            context.clickScreenButton("gui.back");
            context.waitForScreen(LightboundSettingsScreen.class);

            context.clickScreenButton("lightbound.settings.experimental");
            context.waitForScreen(LightboundExperimentalScreen.class);
            context.runOnClient(client -> checkLayout(client, 320, 240));
            context.clickScreenButton("Curved edges: OFF");
            require(ClientVisualSettings.curvedEdges, "Curved contours must toggle live");
            click(context, "gatewayVoid", 0.5);
            require(ClientVisualSettings.gatewayVoid, "Experimental rainbow void must toggle live");
            context.takeScreenshot(TestScreenshotOptions.of("lightbound-experimental-menu-minimum").withSize(320, 240));
            context.clickScreenButton("gui.back");
            context.waitForScreen(LightboundSettingsScreen.class);

            click(context, "dynamicLighting", 0.5);
            require(!ClientVisualSettings.dynamicLighting, "Dynamic lighting toggle must apply immediately");
            click(context, "binaryLighting", 0.5);
            require(!ClientVisualSettings.binaryLighting, "Uniform lighting toggle must apply immediately");
            context.runOnClient(client -> require(!widget(client, "litBrightness").active,
                    "Brightness control must be disabled without uniform lighting"));
            click(context, "binaryLighting", 0.5);
            click(context, "litBrightness", 0.25);
            require(ClientVisualSettings.litBrightness > 0.4 && ClientVisualSettings.litBrightness < 0.65,
                    "Native brightness slider must apply mouse input immediately");
            context.runOnClient(client -> client.gui.screen().setFocused(widget(client, "edgeGlow")));
            context.getInput().pressKey(InputConstants.KEY_SPACE);
            require(!ClientVisualSettings.edgeGlow, "A focused toggle must accept keyboard activation");
            click(context, "particleDensity", 0.75);
            require(ClientVisualSettings.particleDensity > 0.6 && ClientVisualSettings.particleDensity < 0.9,
                    "Native density slider must apply mouse input immediately");
            require(context.computeOnClient(client -> LightboundSettings.threshold(client.level)) == threshold,
                    "Visual options must never change the server's solidity threshold");

            Snapshot edited = context.computeOnClient(client -> Snapshot.capture());
            context.getInput().pressKey(InputConstants.KEY_ESCAPE);
            context.waitFor(client -> client.gui.screen() == null);
            context.runOnClient(client -> {
                ClientVisualSettings.resetDefaults();
                ClientVisualSettings.reloadFromDisk();
                require(edited.equals(Snapshot.capture()), "Closing the menu must persist all live edits");
            });
            context.getInput().resizeWindow(1280, 720);
            context.runOnClient(client -> {
                client.options.guiScale().set(2);
                client.resizeGui();
            });
            context.setScreen(() -> new PauseScreen(true));
            context.clickScreenButton("lightbound.settings.title");
            context.waitForScreen(LightboundSettingsScreen.class);
            context.runOnClient(client -> checkLayout(client, 640, 360));
            context.takeScreenshot(TestScreenshotOptions.of("lightbound-menu-laptop").withSize(1280, 720));
            context.clickScreenButton("lightbound.settings.gameplay");
            context.waitForScreen(LightboundGameplayScreen.class);
            context.runOnClient(client -> checkLayout(client, 640, 360));
            context.takeScreenshot(TestScreenshotOptions.of("lightbound-gameplay-laptop").withSize(1280, 720));
            context.clickScreenButton("gui.back");
            context.waitForScreen(LightboundSettingsScreen.class);
            context.clickScreenButton("lightbound.settings.glowColor.open");
            context.waitForScreen(LightboundGlowColorScreen.class);
            context.runOnClient(client -> checkLayout(client, 640, 360));
            context.takeScreenshot(TestScreenshotOptions.of("lightbound-glow-color-laptop").withSize(1280, 720));
            context.clickScreenButton("gui.back");
            context.waitForScreen(LightboundSettingsScreen.class);
            context.clickScreenButton("lightbound.settings.reset");
            context.runOnClient(client -> require(Snapshot.defaults().equals(Snapshot.capture()),
                    "Reset defaults must restore every visual option"));
            context.clickScreenButton("gui.done");
            context.waitForScreen(PauseScreen.class);
        } finally {
            context.runOnClient(client -> {
                client.gui.setScreen(null);
                original.restore();
                ClientVisualSettings.save();
                client.options.guiScale().set(window[2]);
                client.resizeGui();
            });
            context.getInput().resizeWindow(window[0], window[1]);
        }
    }

    private static void checkLayout(Minecraft client, int width, int height) {
        var screen = client.gui.screen();
        boolean experimental = screen instanceof LightboundExperimentalScreen;
        boolean color = screen instanceof LightboundGlowColorScreen;
        boolean gameplay = screen instanceof LightboundGameplayScreen;
        require(experimental || color || gameplay || screen instanceof LightboundSettingsScreen,
                "Expected a native Uncommon Nights menu");
        if (screen instanceof LightboundSettingsScreen) {
            require(screen.getTitle().getString().equals("Uncommon Nights"), "Settings must show the new mod name");
        }
        require(screen.width == width && screen.height == height,
                "Unexpected GUI size: " + screen.width + "x" + screen.height);
        require(!screen.isPauseScreen(), "Live appearance screen must not pause");
        List<AbstractWidget> widgets = Screens.getWidgets(screen);
        require(widgets.size() == (experimental ? 3 : color ? 5 : gameplay ? 9 : 12),
                "All settings and navigation buttons must be reachable");
        for (int i = 0; i < widgets.size(); i++) {
            AbstractWidget current = widgets.get(i);
            require(current.getX() >= 0 && current.getY() >= 0
                            && current.getX() + current.getWidth() <= width
                            && current.getY() + current.getHeight() <= height,
                    "Widget extends beyond the screen: " + current.getMessage().getString());
            require(current.getHeight() >= 20 && current.getWidth() >= 80,
                    "Native controls need comfortable hit targets");
            require(client.font.width(current.getMessage()) <= current.getWidth() - 8,
                    "Label must fit without scrolling: " + current.getMessage().getString());
            for (int j = i + 1; j < widgets.size(); j++) {
                AbstractWidget other = widgets.get(j);
                require(current.getX() + current.getWidth() <= other.getX()
                                || other.getX() + other.getWidth() <= current.getX()
                                || current.getY() + current.getHeight() <= other.getY()
                                || other.getY() + other.getHeight() <= current.getY(),
                        "Settings hit targets overlap");
            }
        }
    }

    private static void click(ClientGameTestContext context, String name, double fraction) {
        double[] point = context.computeOnClient(client -> {
            AbstractWidget control = widget(client, name);
            var screen = client.gui.screen();
            return new double[]{(control.getX() + control.getWidth() * fraction)
                    * client.getWindow().getScreenWidth() / screen.width,
                    (control.getY() + control.getHeight() / 2.0)
                            * client.getWindow().getScreenHeight() / screen.height};
        });
        context.getInput().setCursorPos(point[0], point[1]);
        context.getInput().pressMouse(InputConstants.MOUSE_BUTTON_LEFT);
        context.waitTick();
    }

    private static AbstractWidget widget(Minecraft client, String name) {
        String prefix = Component.translatable("lightbound.settings." + name).getString() + ":";
        return Screens.getWidgets(client.gui.screen()).stream()
                .filter(control -> control.getMessage().getString().startsWith(prefix))
                .findFirst().orElseThrow(() -> new AssertionError("Missing control: " + name));
    }

    private static void checkSwatch(Path screenshot, int expected) {
        try {
            int painted = ImageIO.read(screenshot.toFile()).getRGB(280, 76);
            for (int shift : new int[]{16, 8, 0}) {
                require(Math.abs(((painted >> shift) & 255) - ((expected >> shift) & 255)) <= 12,
                        "The preview swatch must display the selected RGB color");
            }
        } catch (IOException exception) {
            throw new AssertionError("Could not inspect the native color swatch", exception);
        }
    }

    private static void verifyPersistence() {
        Path path = FabricLoader.getInstance().getConfigDir().resolve("lightbound-client.json");
        Snapshot original = Snapshot.capture();
        try {
            byte[] previousFile = Files.exists(path) ? Files.readAllBytes(path) : null;
            try {
                Snapshot expected = new Snapshot(false, false, false, false, true, 0x32B8D9, 0.63, 1.72, 0.24);
                long before = ClientVisualSettings.revision;
                expected.restore();
                require(ClientVisualSettings.revision > before, "Live edits must invalidate rendering caches");
                ClientVisualSettings.save();
                JsonObject saved = JsonParser.parseString(Files.readString(path)).getAsJsonObject();
                require(saved.size() == 10 && saved.get("litBrightness").getAsDouble() == 0.63
                                && saved.get("glowColor").getAsInt() == 0x32B8D9,
                        "JSON must contain the actual edited settings");
                ClientVisualSettings.resetDefaults();
                ClientVisualSettings.reloadFromDisk();
                require(expected.equals(Snapshot.capture()), "Appearance settings must survive a disk roundtrip");

                Files.writeString(path, """
                        {"dynamicLighting":"false","binaryLighting":null,"edgeGlow":3,
                         "auraParticles":false,"gatewayVoid":"true","glowColor":99999999,
                         "litBrightness":-10,"glowStrength":99,"particleDensity":1e999}
                        """);
                ClientVisualSettings.reloadFromDisk();
                require(new Snapshot(true, true, true, false, false, 0xFFFFFF, 0.35, 2.0, 0.5).equals(Snapshot.capture()),
                        "Invalid types/nonfinite values must default; finite numbers must clamp safely");

                Files.writeString(path, "{\"glowColor\":1.5}");
                ClientVisualSettings.reloadFromDisk();
                require(ClientVisualSettings.glowColor == 0xFFB52E, "Fractional RGB values must use the default gold");

                ClientVisualSettings.litBrightness = Double.NaN;
                ClientVisualSettings.glowStrength = Double.POSITIVE_INFINITY;
                ClientVisualSettings.particleDensity = -1.0;
                ClientVisualSettings.glowColor = -1;
                ClientVisualSettings.changed();
                ClientVisualSettings.save();
                ClientVisualSettings.reloadFromDisk();
                require(ClientVisualSettings.litBrightness == 0.85 && ClientVisualSettings.glowStrength == 1.0
                                && ClientVisualSettings.particleDensity == 0.0 && ClientVisualSettings.glowColor == 0,
                        "Invalid live values must never enter the saved JSON");
            } finally {
                original.restore();
                ClientVisualSettings.save();
                if (previousFile == null) Files.deleteIfExists(path);
                else Files.write(path, previousFile);
            }
        } catch (IOException exception) {
            throw new AssertionError("Could not validate persisted visual settings", exception);
        }
    }

    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }

    private record Snapshot(boolean dynamicLighting, boolean binaryLighting, boolean edgeGlow, boolean auraParticles,
                            boolean gatewayVoid, int glowColor,
                            double litBrightness, double glowStrength, double particleDensity) {
        static Snapshot capture() {
            return new Snapshot(ClientVisualSettings.dynamicLighting, ClientVisualSettings.binaryLighting,
                    ClientVisualSettings.edgeGlow, ClientVisualSettings.auraParticles, ClientVisualSettings.gatewayVoid,
                    ClientVisualSettings.glowColor,
                    ClientVisualSettings.litBrightness,
                    ClientVisualSettings.glowStrength, ClientVisualSettings.particleDensity);
        }

        static Snapshot defaults() {
            return new Snapshot(true, true, true, true, false, 0xFFB52E, 0.85, 1.0, 0.5);
        }

        void restore() {
            ClientVisualSettings.dynamicLighting = dynamicLighting;
            ClientVisualSettings.binaryLighting = binaryLighting;
            ClientVisualSettings.edgeGlow = edgeGlow;
            ClientVisualSettings.auraParticles = auraParticles;
            ClientVisualSettings.gatewayVoid = gatewayVoid;
            ClientVisualSettings.glowColor = glowColor;
            ClientVisualSettings.litBrightness = litBrightness;
            ClientVisualSettings.glowStrength = glowStrength;
            ClientVisualSettings.particleDensity = particleDensity;
            ClientVisualSettings.changed();
        }
    }
}
