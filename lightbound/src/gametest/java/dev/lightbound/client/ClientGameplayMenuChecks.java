package dev.lightbound.client;

import com.mojang.blaze3d.platform.InputConstants;
import dev.lightbound.LightboundSettings;
import java.util.List;
import net.fabricmc.fabric.api.client.gametest.v1.context.ClientGameTestContext;
import net.fabricmc.fabric.api.client.gametest.v1.context.TestSingleplayerContext;
import net.fabricmc.fabric.api.client.screen.v1.Screens;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.AbstractWidget;
import net.minecraft.network.chat.Component;
import net.minecraft.server.permissions.PermissionSet;
import net.minecraft.server.permissions.Permissions;

/** Exercises actual native controls through the server request and synchronization path. */
@SuppressWarnings("UnstableApiUsage")
public final class ClientGameplayMenuChecks {
    private ClientGameplayMenuChecks() {}

    public static void run(ClientGameTestContext context, TestSingleplayerContext world) {
        var server = world.getServer();
        var connection = world.getConnection();
        var original = server.computeOnServer(instance -> LightboundSettings.snapshot(connection.getServerPlayer().level()));
        int originalDarkTicks = server.computeOnServer(instance -> connection.getServerPlayer().level()
                .getGameRules().get(LightboundSettings.HEARTBEAT_DARK_TICKS));
        boolean promoted = server.computeOnServer(instance -> !connection.getServerPlayer().permissions()
                .hasPermission(Permissions.COMMANDS_GAMEMASTER));
        PermissionSet originalClientPermissions = context.computeOnClient(client -> client.player.permissions());
        long originalVisualRevision = ClientVisualSettings.revision;
        try {
            server.runOnServer(instance -> {
                var player = connection.getServerPlayer();
                if (promoted) instance.getPlayerList().op(player.nameAndId());
                var rules = player.level().getGameRules();
                rules.set(LightboundSettings.MINIMUM_LIGHT, 1, instance);
                rules.set(LightboundSettings.AFTERGLOW_TICKS, 0, instance);
                rules.set(LightboundSettings.HEARTBEAT_PERIOD_TICKS, 0, instance);
                rules.set(LightboundSettings.HEARTBEAT_DARK_TICKS, 10, instance);
                rules.set(LightboundSettings.DARK_SWIMMING, false, instance);
                rules.set(LightboundSettings.VOID_LOOP, false, instance);
            });
            connection.waitForClientboundPackets();
            context.waitFor(client -> client.player.permissions().hasPermission(Permissions.COMMANDS_GAMEMASTER)
                    && LightboundSettings.threshold(client.level) == 1
                    && LightboundSettings.afterglowTicks(client.level) == 0
                    && LightboundSettings.heartbeatPeriodTicks(client.level) == 0
                    && !LightboundSettings.darkSwimming(client.level)
                    && !LightboundSettings.voidLoop(client.level));
            context.setScreen(() -> new LightboundGameplayScreen(null));
            context.waitForScreen(LightboundGameplayScreen.class);
            context.runOnClient(client -> {
                require(widget(client, "threshold").active, "Operators must be able to edit the solidity threshold");
                require(widget(client, "afterglow").active, "Operators must be able to edit gameplay settings");
                require(!widget(client, "period").active && !widget(client, "blackout").active,
                        "Heartbeat timing must be disabled while heartbeat is off");
            });

            click(context, "threshold", 0.5);
            context.waitFor(client -> LightboundSettings.threshold(client.level) == 8);
            click(context, "afterglow", 0.3);
            context.waitFor(client -> LightboundSettings.afterglowTicks(client.level) >= 50
                    && LightboundSettings.afterglowTicks(client.level) <= 70);
            click(context, "heartbeat", 0.5);
            context.waitFor(client -> LightboundSettings.heartbeatPeriodTicks(client.level) == 80);
            context.waitTick();
            click(context, "period", 0.1);
            context.waitFor(client -> LightboundSettings.heartbeatPeriodTicks(client.level) > 80
                    && LightboundSettings.heartbeatPeriodTicks(client.level) < 120);
            context.waitTick();
            click(context, "blackout", 0.25);
            context.waitFor(client -> LightboundSettings.heartbeatDarkTicks(client.level) > 15
                    && LightboundSettings.heartbeatDarkTicks(client.level) < 35);
            click(context, "swimming", 0.5);
            context.waitFor(client -> LightboundSettings.darkSwimming(client.level));
            click(context, "voidLoop", 0.5);
            context.waitFor(client -> LightboundSettings.voidLoop(client.level));
            var accepted = context.computeOnClient(client -> LightboundSettings.snapshot(client.level));
            server.runOnServer(instance -> require(accepted.equals(LightboundSettings.snapshot(connection.getServerPlayer().level())),
                    "Every gameplay control must change server rules and return the same synchronized snapshot"));
            require(accepted.minimumLight() == 8, "The native threshold slider must change the authoritative server rule");
            require(ClientVisualSettings.revision == originalVisualRevision,
                    "Gameplay controls must not write client-only appearance settings");

            context.runOnClient(client -> client.player.setPermissions(PermissionSet.NO_PERMISSIONS));
            context.waitTick();
            context.runOnClient(client -> require(widget(client, "mineUnlit").active,
                    "The actual host remains authorized independently of operator status"));
            click(context, "mineUnlit", 0.5);
            context.waitFor(client -> LightboundSettings.mineUnlit(client.level));
            server.runOnServer(instance -> require(LightboundSettings.mineUnlit(connection.getServerPlayer().level()),
                    "The host mining toggle must reach the server"));
            context.clickScreenButton("gui.back");
        } finally {
            context.runOnClient(client -> client.gui.setScreen(null));
            server.runOnServer(instance -> {
                var player = connection.getServerPlayer();
                var rules = player.level().getGameRules();
                rules.set(LightboundSettings.MINIMUM_LIGHT, original.minimumLight(), instance);
                rules.set(LightboundSettings.AFTERGLOW_TICKS, original.afterglowTicks(), instance);
                rules.set(LightboundSettings.HEARTBEAT_PERIOD_TICKS, original.heartbeatPeriodTicks(), instance);
                rules.set(LightboundSettings.HEARTBEAT_DARK_TICKS, originalDarkTicks, instance);
                rules.set(LightboundSettings.DARK_SWIMMING, original.darkSwimming(), instance);
                rules.set(LightboundSettings.VOID_LOOP, original.voidLoop(), instance);
                rules.set(LightboundSettings.MINE_UNLIT, original.mineUnlit(), instance);
                if (promoted) instance.getPlayerList().deop(player.nameAndId());
            });
            connection.waitForClientboundPackets();
            context.waitFor(client -> original.equals(LightboundSettings.snapshot(client.level)));
            context.runOnClient(client -> client.player.setPermissions(originalClientPermissions));
        }
    }

    private static AbstractWidget widget(Minecraft client, String name) {
        String prefix = Component.translatable("lightbound.gameplay." + name).getString() + ":";
        return Screens.getWidgets(client.gui.screen()).stream()
                .filter(control -> control.getMessage().getString().startsWith(prefix))
                .findFirst().orElseThrow(() -> new AssertionError("Missing gameplay control: " + name));
    }

    private static void click(ClientGameTestContext context, String name, double fraction) {
        double[] position = context.computeOnClient(client -> {
            var control = widget(client, name);
            var screen = client.gui.screen();
            return new double[]{(control.getX() + control.getWidth() * fraction)
                    * client.getWindow().getScreenWidth() / screen.width,
                    (control.getY() + control.getHeight() / 2.0)
                            * client.getWindow().getScreenHeight() / screen.height};
        });
        context.getInput().setCursorPos(position[0], position[1]);
        context.getInput().pressMouse(InputConstants.MOUSE_BUTTON_LEFT);
        context.waitTick();
    }

    private static void require(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
}
