package dev.lightbound.client;

import dev.lightbound.LightboundSettings;
import dev.lightbound.LightboundSettings.Setting;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.function.BooleanSupplier;
import java.util.function.IntSupplier;
import java.util.function.IntUnaryOperator;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayNetworking;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.AbstractSliderButton;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.components.Tooltip;
import net.minecraft.client.gui.screens.Screen;
import net.minecraft.client.input.MouseButtonEvent;
import net.minecraft.network.chat.Component;
import net.minecraft.server.permissions.Permissions;

/** Edits world rules through permission-checked server requests, never local gameplay state. */
public final class LightboundGameplayScreen extends Screen {
    private static final int PANEL_HEIGHT = 204;
    private final Screen parent;
    private final List<ServerSlider> sliders = new ArrayList<>();
    private final List<ServerToggle> toggles = new ArrayList<>();
    private int panelX;
    private int panelY;
    private int panelWidth;
    private int age;
    private int lastHeartbeatPeriod = 80;

    public LightboundGameplayScreen(Screen parent) {
        super(Component.translatable("lightbound.gameplay.title"));
        this.parent = parent;
    }

    @Override
    protected void init() {
        sliders.forEach(ServerSlider::sendPending);
        sliders.clear();
        toggles.clear();
        panelWidth = Math.min(296, width - 16);
        panelX = (width - panelWidth) / 2;
        panelY = Math.max(4, (height - PANEL_HEIGHT) / 2);
        int x = panelX + 12;
        int y = panelY + 44;
        int full = panelWidth - 24;
        int half = (full - 4) / 2;
        int currentPeriod = LightboundSettings.heartbeatPeriodTicks(minecraft.level);
        if (currentPeriod > 0) lastHeartbeatPeriod = currentPeriod;

        slider(x, y, full, "threshold", Setting.MINIMUM_LIGHT,
                () -> LightboundSettings.threshold(minecraft.level), 1, () -> 15, () -> true);
        slider(x, y + 22, full, "afterglow", Setting.AFTERGLOW_TICKS,
                () -> LightboundSettings.afterglowTicks(minecraft.level), 0, () -> 200, () -> true);
        toggle(x, y + 44, full, "heartbeat", Setting.HEARTBEAT_PERIOD_TICKS,
                () -> LightboundSettings.heartbeatPeriodTicks(minecraft.level),
                enabled -> enabled == 0 ? 0 : lastHeartbeatPeriod);
        slider(x, y + 66, half, "period", Setting.HEARTBEAT_PERIOD_TICKS, () -> {
            int period = LightboundSettings.heartbeatPeriodTicks(minecraft.level);
            if (period > 0) lastHeartbeatPeriod = period;
            return lastHeartbeatPeriod;
        }, 1, () -> 1200, this::heartbeatEnabled);
        slider(x + half + 4, y + 66, full - half - 4, "blackout", Setting.HEARTBEAT_DARK_TICKS,
                () -> LightboundSettings.heartbeatDarkTicks(minecraft.level), 1,
                () -> Math.min(200, heartbeatEnabled() ? LightboundSettings.heartbeatPeriodTicks(minecraft.level) : 200),
                this::heartbeatEnabled);
        toggle(x, y + 88, half, "swimming", Setting.DARK_SWIMMING,
                () -> LightboundSettings.darkSwimming(minecraft.level) ? 1 : 0, value -> value);
        toggle(x + half + 4, y + 88, full - half - 4, "voidLoop", Setting.VOID_LOOP,
                () -> LightboundSettings.voidLoop(minecraft.level) ? 1 : 0, value -> value);
        toggle(x, y + 110, full, "mineUnlit", Setting.MINE_UNLIT,
                () -> LightboundSettings.mineUnlit(minecraft.level) ? 1 : 0, value -> value);
        addRenderableWidget(Button.builder(Component.translatable("gui.back"), button -> onClose())
                .bounds(x, panelY + 178, full, 20).build());
        refreshControls();
        if (canSend()) ClientPlayNetworking.send(new LightboundSettings.SettingsRequestPayload(Setting.REFRESH, 0));
    }

    private boolean heartbeatEnabled() {
        return LightboundSettings.heartbeatPeriodTicks(minecraft.level) > 0;
    }

    private boolean canSend() {
        return minecraft.level != null && minecraft.getConnection() != null
                && ClientPlayNetworking.canSend(LightboundSettings.SettingsRequestPayload.TYPE);
    }

    private boolean canEdit() {
        return canSend() && minecraft.player != null
                && minecraft.hasSingleplayerServer() && minecraft.getSingleplayerServer().isSingleplayerOwner(minecraft.player.nameAndId());
    }

    private boolean send(Setting setting, int value) {
        if (!canEdit()) return false;
        ClientPlayNetworking.send(new LightboundSettings.SettingsRequestPayload(setting, value));
        return true;
    }

    private void slider(int x, int y, int width, String name, Setting setting, IntSupplier reader,
                        int min, IntSupplier max, BooleanSupplier available) {
        ServerSlider slider = new ServerSlider(x, y, width, name, setting, reader, min, max, available);
        sliders.add(slider);
        addRenderableWidget(slider);
    }

    private void toggle(int x, int y, int width, String name, Setting setting, IntSupplier reader,
                        IntUnaryOperator valueForState) {
        ServerToggle toggle = new ServerToggle(name, setting, reader, valueForState);
        toggle.button = addRenderableWidget(Button.builder(toggle.label(), button -> toggle.press())
                .bounds(x, y, width, 20).tooltip(tooltip(name)).build());
        toggles.add(toggle);
    }

    @Override
    public void tick() {
        age++;
        refreshControls();
    }

    private void refreshControls() {
        boolean editable = canEdit();
        sliders.forEach(slider -> slider.refresh(editable));
        toggles.forEach(toggle -> toggle.refresh(editable));
    }

    private static Component label(String name, Object value) {
        return Component.translatable("lightbound.settings.value",
                Component.translatable("lightbound.gameplay." + name), value);
    }

    private static Tooltip tooltip(String name) {
        return Tooltip.create(Component.translatable("lightbound.gameplay." + name + ".tooltip"));
    }

    private static Component seconds(int ticks) {
        if (ticks == 0) return Component.translatable("options.off");
        String value = ticks % 20 == 0 ? Integer.toString(ticks / 20)
                : String.format(Locale.ROOT, "%.2f", ticks / 20.0).replaceFirst("0$", "");
        return Component.translatable("lightbound.gameplay.seconds", value);
    }

    @Override
    public void extractRenderState(GuiGraphicsExtractor graphics, int mouseX, int mouseY, float partialTick) {
        graphics.fill(panelX - 1, panelY - 1, panelX + panelWidth + 1, panelY + PANEL_HEIGHT + 1, 0xff596475);
        graphics.fill(panelX, panelY, panelX + panelWidth, panelY + PANEL_HEIGHT, 0xf51c2028);
        graphics.fill(panelX, panelY, panelX + panelWidth, panelY + 2, 0xff85adff);
        graphics.centeredText(font, title, panelX + panelWidth / 2, panelY + 12, 0xfff2f4ff);
        graphics.centeredText(font, Component.translatable(canEdit()
                        ? "lightbound.gameplay.server" : "lightbound.gameplay.readOnly"),
                panelX + panelWidth / 2, panelY + 28, 0xffaeb8ca);
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
        sliders.forEach(ServerSlider::sendPending);
    }

    private final class ServerToggle {
        private final String name;
        private final Setting setting;
        private final IntSupplier reader;
        private final IntUnaryOperator valueForState;
        private Button button;
        private int expected = -1;
        private int sentAt;

        ServerToggle(String name, Setting setting, IntSupplier reader, IntUnaryOperator valueForState) {
            this.name = name;
            this.setting = setting;
            this.reader = reader;
            this.valueForState = valueForState;
        }

        Component label() {
            boolean enabled = (expected >= 0 ? expected : reader.getAsInt()) > 0;
            return LightboundGameplayScreen.label(name, Component.translatable(enabled ? "options.on" : "options.off"));
        }

        void press() {
            int target = valueForState.applyAsInt((expected >= 0 ? expected : reader.getAsInt()) > 0 ? 0 : 1);
            if (send(setting, target)) {
                expected = target;
                sentAt = age;
                button.setMessage(label());
            }
        }

        void refresh(boolean editable) {
            button.active = editable;
            if (!editable || reader.getAsInt() == expected || age - sentAt >= 40) expected = -1;
            button.setMessage(label());
        }
    }

    private final class ServerSlider extends AbstractSliderButton {
        private final String name;
        private final Setting setting;
        private final IntSupplier reader;
        private final int min;
        private final IntSupplier max;
        private final BooleanSupplier available;
        private int pending = -1;
        private int expected = -1;
        private int sentAt = -4;
        private boolean dragging;

        ServerSlider(int x, int y, int width, String name, Setting setting, IntSupplier reader,
                     int min, IntSupplier max, BooleanSupplier available) {
            super(x, y, width, 20, Component.empty(), 0.0);
            this.name = name;
            this.setting = setting;
            this.reader = reader;
            this.min = min;
            this.max = max;
            this.available = available;
            readServer();
            setTooltip(tooltip(name));
        }

        private int selectedValue() {
            int limit = Math.max(min, max.getAsInt());
            return min + (int) Math.round(value * (limit - min));
        }

        private void readServer() {
            int limit = Math.max(min, max.getAsInt());
            int current = Math.clamp(reader.getAsInt(), min, limit);
            value = limit == min ? 0.0 : (double) (current - min) / (limit - min);
            updateMessage();
        }

        void refresh(boolean editable) {
            active = editable && available.getAsBoolean();
            if (!active) {
                pending = -1;
                expected = -1;
                dragging = false;
                readServer();
                return;
            }
            if (pending >= 0 && age - sentAt >= 4) sendPending();
            if (!dragging && pending < 0 && (expected < 0 || reader.getAsInt() == expected || age - sentAt >= 40)) {
                expected = -1;
                readServer();
            }
        }

        @Override
        protected void updateMessage() {
            int selected = selectedValue();
            setMessage(label(name, setting == Setting.MINIMUM_LIGHT
                    ? Component.literal(Integer.toString(selected)) : seconds(selected)));
        }

        @Override
        protected void applyValue() {
            pending = selectedValue();
            if (age - sentAt >= 4) sendPending();
        }

        void sendPending() {
            if (pending < 0) return;
            int target = pending;
            pending = -1;
            if (send(setting, target)) {
                expected = target;
                sentAt = age;
            }
        }

        @Override
        public void onClick(MouseButtonEvent event, boolean doubleClick) {
            dragging = true;
            super.onClick(event, doubleClick);
        }

        @Override
        public void onRelease(MouseButtonEvent event) {
            super.onRelease(event);
            dragging = false;
            sendPending();
        }
    }
}
