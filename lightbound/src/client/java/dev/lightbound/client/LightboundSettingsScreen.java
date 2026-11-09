package dev.lightbound.client;

import dev.lightbound.LightboundSettings;
import java.util.function.BooleanSupplier;
import java.util.function.Consumer;
import java.util.function.DoubleConsumer;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractSliderButton;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;

/** A compact live preview panel; none of these options change server collision. */
public final class LightboundSettingsScreen extends Screen {
    private final Screen parent;
    private int panelX;
    private int panelY;
    private int panelWidth;
    private static final int PANEL_HEIGHT = 204;
    private ValueSlider brightnessSlider;
    private ValueSlider glowSlider;
    private ValueSlider densitySlider;

    public LightboundSettingsScreen(Screen parent) {
        super(Component.translatable("lightbound.settings.title"));
        this.parent = parent;
    }

    @Override
    protected void init() {
        panelWidth = Math.min(296, width - 16);
        panelX = (width - panelWidth) / 2;
        panelY = Math.max(4, (height - PANEL_HEIGHT) / 2);
        int x = panelX + 12;
        int y = panelY + 42;
        int full = panelWidth - 24;
        int half = (full - 4) / 2;

        toggle(x, y, full, "dynamicLighting", () -> ClientVisualSettings.dynamicLighting,
                value -> ClientVisualSettings.dynamicLighting = value);
        toggle(x, y + 22, full, "binaryLighting", () -> ClientVisualSettings.binaryLighting,
                value -> ClientVisualSettings.binaryLighting = value);
        int colorButtonWidth = 90;
        brightnessSlider = slider(x, y + 44, full - colorButtonWidth - 4, "litBrightness", ClientVisualSettings.litBrightness,
                0.35, 1.0, value -> ClientVisualSettings.litBrightness = value);
        addRenderableWidget(Button.builder(Component.translatable("lightbound.settings.glowColor.open"),
                        button -> minecraft.gui.setScreen(new LightboundGlowColorScreen(this)))
                .bounds(x + full - colorButtonWidth, y + 44, colorButtonWidth, 20)
                .tooltip(tooltip("glowColor")).build());
        toggle(x, y + 66, half, "edgeGlow", () -> ClientVisualSettings.edgeGlow,
                value -> ClientVisualSettings.edgeGlow = value);
        glowSlider = slider(x + half + 4, y + 66, full - half - 4, "glowStrength",
                ClientVisualSettings.glowStrength, 0.0, 2.0, value -> ClientVisualSettings.glowStrength = value);
        toggle(x, y + 88, half, "auraParticles", () -> ClientVisualSettings.auraParticles,
                value -> ClientVisualSettings.auraParticles = value);
        densitySlider = slider(x + half + 4, y + 88, full - half - 4, "particleDensity",
                ClientVisualSettings.particleDensity, 0.0, 1.0, value -> ClientVisualSettings.particleDensity = value);

        addRenderableWidget(Button.builder(Component.translatable("lightbound.settings.gameplay"),
                        button -> minecraft.gui.setScreen(new LightboundGameplayScreen(this)))
                .bounds(x, panelY + 154, full, 20).build());

        int footerWidth = (full - 8) / 3;
        addRenderableWidget(Button.builder(Component.translatable("lightbound.settings.reset"), button -> {
            ClientVisualSettings.resetDefaults();
            rebuildWidgets();
        }).bounds(x, panelY + 178, footerWidth, 20).build());
        addRenderableWidget(Button.builder(Component.translatable("lightbound.settings.experimental"),
                        button -> minecraft.gui.setScreen(new LightboundExperimentalScreen(this)))
                .bounds(x + footerWidth + 4, panelY + 178, footerWidth, 20).build());
        addRenderableWidget(Button.builder(Component.translatable("gui.done"), button -> onClose())
                .bounds(x + 2 * (footerWidth + 4), panelY + 178, full - 2 * (footerWidth + 4), 20).build());
        updateAvailability();
    }

    private void toggle(int x, int y, int width, String name, BooleanSupplier getter, Consumer<Boolean> setter) {
        addRenderableWidget(Button.builder(toggleLabel(name, getter.getAsBoolean()), button -> {
            setter.accept(!getter.getAsBoolean());
            button.setMessage(toggleLabel(name, getter.getAsBoolean()));
            ClientVisualSettings.changed();
            updateAvailability();
        }).bounds(x, y, width, 20).tooltip(tooltip(name)).build());
    }

    private ValueSlider slider(int x, int y, int width, String name, double value, double min, double max,
                               DoubleConsumer setter) {
        return addRenderableWidget(new ValueSlider(x, y, width, name, value, min, max, setter));
    }

    private void updateAvailability() {
        if (brightnessSlider != null) brightnessSlider.active = ClientVisualSettings.binaryLighting;
        if (glowSlider != null) glowSlider.active = ClientVisualSettings.edgeGlow || ClientVisualSettings.auraParticles;
        if (densitySlider != null) densitySlider.active = ClientVisualSettings.auraParticles;
    }

    private static Component toggleLabel(String name, boolean value) {
        return Component.translatable("lightbound.settings.value", Component.translatable("lightbound.settings." + name),
                Component.translatable(value ? "options.on" : "options.off"));
    }

    private static Tooltip tooltip(String name) {
        return Tooltip.create(Component.translatable("lightbound.settings." + name + ".tooltip"));
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partialTick) {
        graphics.fill(panelX - 1, panelY - 1, panelX + panelWidth + 1, panelY + PANEL_HEIGHT + 1, 0xff596475);
        graphics.fill(panelX, panelY, panelX + panelWidth, panelY + PANEL_HEIGHT, 0xf51c2028);
        graphics.fill(panelX, panelY, panelX + panelWidth, panelY + 2, 0xff85adff);
        graphics.centeredText(font, title, panelX + panelWidth / 2, panelY + 12, 0xfff2f4ff);
        graphics.centeredText(font, Component.translatable("lightbound.settings.serverThreshold",
                        LightboundSettings.threshold(minecraft.level)),
                panelX + panelWidth / 2, panelY + 28, 0xffaeb8ca);
        super.extractRenderState(graphics, mouseX, mouseY, partialTick);
    }

    @Override
    public void extractBackground(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partialTick) {
        // Keep the world unobscured so light changes remain visible while adjusting sliders.
    }

    @Override
    public boolean isPauseScreen() {
        return false;
    }

    @Override
    public boolean isInGameUi() {
        return true;
    }

    @Override
    public void onClose() {
        minecraft.gui.setScreen(parent);
    }

    @Override
    public void removed() {
        ClientVisualSettings.save();
    }

    private static final class ValueSlider extends AbstractSliderButton {
        private final String name;
        private final double min;
        private final double max;
        private final DoubleConsumer setter;

        ValueSlider(int x, int y, int width, String name, double initialValue, double min, double max,
                    DoubleConsumer setter) {
            super(x, y, width, 20, Component.empty(), (initialValue - min) / (max - min));
            this.name = name;
            this.min = min;
            this.max = max;
            this.setter = setter;
            setTooltip(tooltip(name));
            updateMessage();
        }

        @Override
        protected void updateMessage() {
            setMessage(Component.translatable("lightbound.settings.value",
                    Component.translatable("lightbound.settings." + name),
                    Component.literal(Math.round((min + value * (max - min)) * 100) + "%")));
        }

        @Override
        protected void applyValue() {
            setter.accept(min + value * (max - min));
            ClientVisualSettings.changed();
        }
    }
}
