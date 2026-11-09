package dev.lightbound.client;

import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.network.chat.Component;

/** Optional visual experiments, kept separate from the everyday lighting controls. */
public final class LightboundExperimentalScreen extends Screen {
    private static final int PANEL_HEIGHT = 120;
    private final Screen parent;
    private int panelX;
    private int panelY;
    private int panelWidth;

    public LightboundExperimentalScreen(Screen parent) {
        super(Component.translatable("lightbound.experimental.title"));
        this.parent = parent;
    }

    @Override
    protected void init() {
        panelWidth = Math.min(296, width - 16);
        panelX = (width - panelWidth) / 2;
        panelY = Math.max(4, (height - PANEL_HEIGHT) / 2);
        int x = panelX + 12;
        int full = panelWidth - 24;
        addRenderableWidget(Button.builder(toggleLabel(), button -> {
            ClientVisualSettings.gatewayVoid = !ClientVisualSettings.gatewayVoid;
            ClientVisualSettings.changed();
            button.setMessage(toggleLabel());
        }).bounds(x, panelY + 36, full, 20)
                .tooltip(Tooltip.create(Component.translatable("lightbound.settings.gatewayVoid.tooltip"))).build());
        addRenderableWidget(Button.builder(curveLabel(), button -> {
            ClientVisualSettings.curvedEdges = !ClientVisualSettings.curvedEdges;
            ClientVisualSettings.changed();
            button.setMessage(curveLabel());
        }).bounds(x, panelY + 60, full, 20).tooltip(Tooltip.create(Component.literal("Rounds the visible contour; block collision stays unchanged."))).build());
        addRenderableWidget(Button.builder(Component.translatable("gui.back"), button -> onClose())
                .bounds(x, panelY + 88, full, 20).build());
    }

    private static Component curveLabel() {
        return Component.literal("Curved edges: " + (ClientVisualSettings.curvedEdges ? "ON" : "OFF"));
    }

    private static Component toggleLabel() {
        return Component.translatable("lightbound.settings.value",
                Component.translatable("lightbound.settings.gatewayVoid"),
                Component.translatable(ClientVisualSettings.gatewayVoid ? "options.on" : "options.off"));
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partialTick) {
        graphics.fill(panelX - 1, panelY - 1, panelX + panelWidth + 1, panelY + PANEL_HEIGHT + 1, 0xff596475);
        graphics.fill(panelX, panelY, panelX + panelWidth, panelY + PANEL_HEIGHT, 0xf51c2028);
        graphics.fill(panelX, panelY, panelX + panelWidth, panelY + 2, 0xffb69aff);
        graphics.centeredText(font, title, panelX + panelWidth / 2, panelY + 14, 0xfff2f4ff);
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
}
