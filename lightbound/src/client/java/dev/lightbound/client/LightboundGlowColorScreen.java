package dev.lightbound.client;

import java.util.Locale;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractSliderButton;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;

/** Live RGB editing for the boundary glow and its particles. */
public final class LightboundGlowColorScreen extends Screen {
    private static final int PANEL_HEIGHT = 176;
    private final Screen parent;
    private int panelX;
    private int panelY;
    private int panelWidth;

    public LightboundGlowColorScreen(Screen parent) {
        super(Component.translatable("lightbound.settings.glowColor"));
        this.parent = parent;
    }

    @Override
    protected void init() {
        panelWidth = Math.min(296, width - 16);
        panelX = (width - panelWidth) / 2;
        panelY = Math.max(4, (height - PANEL_HEIGHT) / 2);
        int x = panelX + 12;
        int full = panelWidth - 24;
        int half = (full - 4) / 2;
        addRenderableWidget(new ChannelSlider(x, panelY + 64, full, "red", 16));
        addRenderableWidget(new ChannelSlider(x, panelY + 88, full, "green", 8));
        addRenderableWidget(new ChannelSlider(x, panelY + 112, full, "blue", 0));
        addRenderableWidget(Button.builder(Component.translatable("lightbound.settings.glowColor.reset"), button -> {
            ClientVisualSettings.glowColor = 0xFFB52E;
            ClientVisualSettings.changed();
            rebuildWidgets();
        }).bounds(x, panelY + 144, half, 20).build());
        addRenderableWidget(Button.builder(Component.translatable("gui.back"), button -> onClose())
                .bounds(x + half + 4, panelY + 144, full - half - 4, 20).build());
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partialTick) {
        int color = ClientVisualSettings.glowColor;
        graphics.fill(panelX - 1, panelY - 1, panelX + panelWidth + 1, panelY + PANEL_HEIGHT + 1, 0xff596475);
        graphics.fill(panelX, panelY, panelX + panelWidth, panelY + PANEL_HEIGHT, 0xf51c2028);
        graphics.fill(panelX, panelY, panelX + panelWidth, panelY + 2, 0xff000000 | color);
        graphics.centeredText(font, title, panelX + panelWidth / 2, panelY + 12, 0xfff2f4ff);
        graphics.fill(panelX + 11, panelY + 31, panelX + panelWidth - 11, panelY + 57, 0xff8993a5);
        graphics.fill(panelX + 12, panelY + 32, panelX + panelWidth - 12, panelY + 56, 0xff000000 | color);
        int red = (color >> 16) & 255;
        int green = (color >> 8) & 255;
        int blue = color & 255;
        int textColor = 299 * red + 587 * green + 114 * blue >= 140000 ? 0xff101010 : 0xfff5f5f5;
        graphics.text(font, String.format(Locale.ROOT, "#%06X", color), panelX + 20, panelY + 40, textColor, false);
        super.extractRenderState(graphics, mouseX, mouseY, partialTick);
    }

    @Override
    public void extractBackground(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partialTick) {}

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

    private static final class ChannelSlider extends AbstractSliderButton {
        private final String channel;
        private final int shift;

        ChannelSlider(int x, int y, int width, String channel, int shift) {
            super(x, y, width, 20, Component.empty(), ((ClientVisualSettings.glowColor >> shift) & 255) / 255.0);
            this.channel = channel;
            this.shift = shift;
            updateMessage();
        }

        @Override
        protected void updateMessage() {
            setMessage(Component.translatable("lightbound.settings.value",
                    Component.translatable("lightbound.settings.glowColor." + channel),
                    (int) Math.round(value * 255)));
        }

        @Override
        protected void applyValue() {
            int component = (int) Math.round(value * 255);
            ClientVisualSettings.glowColor = (ClientVisualSettings.glowColor & ~(255 << shift)) | (component << shift);
            ClientVisualSettings.changed();
        }
    }
}
