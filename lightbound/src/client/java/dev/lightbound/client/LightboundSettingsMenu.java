package dev.lightbound.client;

import com.mojang.blaze3d.platform.InputConstants;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientLifecycleEvents;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.keymapping.v1.KeyMappingHelper;
import net.fabricmc.fabric.api.client.screen.v1.ScreenEvents;
import net.fabricmc.fabric.api.client.screen.v1.Screens;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.gui.components.Button;
import net.minecraft.client.gui.screens.PauseScreen;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;

/** Native menu access without a dependency on Mod Menu. */
public final class LightboundSettingsMenu {
    private static boolean initialized;

    private LightboundSettingsMenu() {}

    public static void initialize() {
        if (initialized) return;
        initialized = true;
        ClientVisualSettings.initialize();
        KeyMapping key = KeyMappingHelper.registerKeyMapping(new KeyMapping(
                "key.lightbound.settings", InputConstants.KEY_K,
                KeyMapping.Category.register(Identifier.fromNamespaceAndPath("lightbound", "settings"))));
        ClientTickEvents.END_CLIENT_TICK.register(client -> {
            ClientVisualSettings.tickSave();
            while (key.consumeClick()) {
                if (client.level != null && client.gui.screen() == null) {
                    client.gui.setScreen(new LightboundSettingsScreen(null));
                }
            }
        });
        ClientLifecycleEvents.CLIENT_STOPPING.register(client -> ClientVisualSettings.save());
        ScreenEvents.AFTER_INIT.register((client, screen, width, height) -> {
            if (screen instanceof PauseScreen pause && pause.showsPauseMenu()) {
                Screens.getWidgets(screen).add(Button.builder(Component.translatable("lightbound.settings.title"),
                                button -> client.gui.setScreen(new LightboundSettingsScreen(screen)))
                        .bounds(width - 104, 8, 96, 20).build());
            }
        });
    }
}
