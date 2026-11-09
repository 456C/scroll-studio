package dev.lightbound.client;

import dev.lightbound.LightboundSettings;
import dev.lightbound.LightMechanics;
import dev.lightbound.AfterglowSync;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayNetworking;

public final class LightboundClient implements ClientModInitializer {
    @Override
    public void onInitializeClient() {
        LightboundSettingsMenu.initialize();
        GatewayVoidEffect.initialize();
        ClientTickEvents.END_CLIENT_TICK.register(client -> {
            LightMechanics.tickClient(client.level, client.player == null ? -1 : client.player.getId());
            if (client.player != null) {
                LightMechanics.applyDarkSwimming(client.player, client.player.input.keyPresses.jump());
            }
        });
        BuiltinDynamicLighting.initialize();
        StandableOutlineRenderer.initialize();
        ClientPlayNetworking.registerGlobalReceiver(LightboundSettings.SettingsPayload.TYPE,
                (payload, context) -> LightboundSettings.acceptServerSettings(payload));
        ClientPlayNetworking.registerGlobalReceiver(AfterglowSync.HistoryPayload.TYPE,
                (payload, context) -> AfterglowSync.accept(payload));
        ClientPlayConnectionEvents.DISCONNECT.register((handler, client) -> LightboundSettings.resetClientSettings());
    }
}
