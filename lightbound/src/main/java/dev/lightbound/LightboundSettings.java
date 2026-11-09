package dev.lightbound;

import net.fabricmc.fabric.api.gamerule.v1.GameRuleBuilder;
import net.fabricmc.fabric.api.gamerule.v1.GameRuleEvents;
import net.fabricmc.fabric.api.networking.v1.PayloadTypeRegistry;
import net.fabricmc.fabric.api.networking.v1.ServerPlayConnectionEvents;
import net.fabricmc.fabric.api.networking.v1.ServerPlayNetworking;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.permissions.Permissions;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.gamerules.GameRule;
import net.minecraft.world.level.gamerules.GameRuleCategory;

/** The server owns gameplay rules. Every update sends one complete prediction snapshot. */
public final class LightboundSettings {
    public static final int DEFAULT_THRESHOLD = 1;
    public static final GameRule<Integer> MINIMUM_LIGHT = integer("minimum_light", DEFAULT_THRESHOLD, 1, 15);
    public static final GameRule<Integer> AFTERGLOW_TICKS = integer("afterglow_ticks", 48, 0, 200);
    public static final GameRule<Integer> HEARTBEAT_PERIOD_TICKS = integer("heartbeat_period_ticks", 0, 0, 1200);
    public static final GameRule<Integer> HEARTBEAT_DARK_TICKS = integer("heartbeat_dark_ticks", 10, 1, 200);
    public static final GameRule<Boolean> DARK_SWIMMING = bool("dark_swimming", false);
    public static final GameRule<Boolean> VOID_LOOP = bool("void_loop", false);
    public static final GameRule<Boolean> MINE_UNLIT = bool("mine_unlit", false);
    private static final SettingsPayload DEFAULTS = new SettingsPayload(DEFAULT_THRESHOLD, 48, 0, 10, false, false, false);

    private static volatile SettingsPayload clientSettings = DEFAULTS;
    private static volatile boolean clientActive;

    private LightboundSettings() {}

    private static GameRule<Integer> integer(String name, int value, int minimum, int maximum) {
        return GameRuleBuilder.forInteger(value).range(minimum, maximum).category(GameRuleCategory.PLAYER)
                .buildAndRegister(Identifier.fromNamespaceAndPath("lightbound", name));
    }

    private static GameRule<Boolean> bool(String name, boolean value) {
        return GameRuleBuilder.forBoolean(value).category(GameRuleCategory.PLAYER)
                .buildAndRegister(Identifier.fromNamespaceAndPath("lightbound", name));
    }

    public static void initialize() {
        PayloadTypeRegistry.clientboundPlay().register(SettingsPayload.TYPE, SettingsPayload.CODEC);
        PayloadTypeRegistry.serverboundPlay().register(SettingsRequestPayload.TYPE, SettingsRequestPayload.CODEC);
        ServerPlayConnectionEvents.JOIN.register((handler, sender, server) -> sendSettings(handler.player));
        ServerPlayNetworking.registerGlobalReceiver(SettingsRequestPayload.TYPE,
                (payload, context) -> applyRequest(context.player(), payload));
        syncChanges(MINIMUM_LIGHT);
        syncChanges(AFTERGLOW_TICKS);
        syncChanges(HEARTBEAT_PERIOD_TICKS);
        syncChanges(HEARTBEAT_DARK_TICKS);
        syncChanges(DARK_SWIMMING);
        syncChanges(VOID_LOOP);
        syncChanges(MINE_UNLIT);
    }

    private static <T> void syncChanges(GameRule<T> rule) {
        GameRuleEvents.changeCallback(rule).register((value, server) -> {
            if (server != null) broadcast(server);
        });
    }

    private static void broadcast(MinecraftServer server) {
        for (ServerPlayer player : server.getPlayerList().getPlayers()) sendSettings(player);
    }

    private static void sendSettings(ServerPlayer player) {
        if (ServerPlayNetworking.canSend(player, SettingsPayload.TYPE)) {
            ServerPlayNetworking.send(player, snapshot(player.level()));
        }
    }

    public static boolean isActive(Level level) {
        return level instanceof ServerLevel || clientActive;
    }

    public static int threshold(Level level) {
        return level instanceof ServerLevel server ? server.getGameRules().get(MINIMUM_LIGHT) : clientSettings.minimumLight;
    }

    public static int afterglowTicks(Level level) {
        return level instanceof ServerLevel server ? server.getGameRules().get(AFTERGLOW_TICKS) : clientSettings.afterglowTicks;
    }

    public static int heartbeatPeriodTicks(Level level) {
        return level instanceof ServerLevel server ? server.getGameRules().get(HEARTBEAT_PERIOD_TICKS) : clientSettings.heartbeatPeriodTicks;
    }

    public static int heartbeatDarkTicks(Level level) {
        int value = level instanceof ServerLevel server ? server.getGameRules().get(HEARTBEAT_DARK_TICKS) : clientSettings.heartbeatDarkTicks;
        int period = heartbeatPeriodTicks(level);
        return Math.min(value, period > 0 ? period : 200);
    }

    public static boolean darkSwimming(Level level) {
        return level instanceof ServerLevel server ? server.getGameRules().get(DARK_SWIMMING) : clientSettings.darkSwimming;
    }

    public static boolean voidLoop(Level level) {
        return level instanceof ServerLevel server ? server.getGameRules().get(VOID_LOOP) : clientSettings.voidLoop;
    }

    public static boolean mineUnlit(Level level) {
        return level instanceof ServerLevel server ? server.getGameRules().get(MINE_UNLIT) : clientSettings.mineUnlit;
    }

    public static boolean canMine(Level level, net.minecraft.core.BlockPos pos) {
        return !isActive(level) || mineUnlit(level) || LightMath.isSolid(LightRules.getEffectiveLight(level, pos), threshold(level));
    }

    public static SettingsPayload snapshot(Level level) {
        return new SettingsPayload(threshold(level), afterglowTicks(level), heartbeatPeriodTicks(level),
                heartbeatDarkTicks(level), darkSwimming(level), voidLoop(level), mineUnlit(level));
    }

    public static void acceptServerSettings(SettingsPayload settings) {
        clientSettings = settings;
        clientActive = true;
    }

    /** Retained for small integrations which change only the existing threshold. */
    public static void acceptServerSettings(int threshold) {
        SettingsPayload old = clientSettings;
        acceptServerSettings(new SettingsPayload(threshold, old.afterglowTicks, old.heartbeatPeriodTicks,
                old.heartbeatDarkTicks, old.darkSwimming, old.voidLoop, old.mineUnlit));
    }

    public static void resetClientSettings() {
        clientActive = false;
        clientSettings = DEFAULTS;
        LightMechanics.clearClient();
    }

    /** Packet permissions are checked on the server even if a client enables disabled controls. */
    public static boolean applyRequest(ServerPlayer player, SettingsRequestPayload request) {
        Setting[] settings = Setting.values();
        if (request.settingId < 0 || request.settingId >= settings.length) return false;
        Setting setting = settings[request.settingId];
        if (setting == Setting.REFRESH) {
            sendSettings(player);
            return true;
        }
        if (!player.level().getServer().isSingleplayerOwner(player.nameAndId())) {
            sendSettings(player);
            return false;
        }
        ServerLevel level = player.level();
        var rules = level.getGameRules();
        var server = level.getServer();
        int value = request.value;
        switch (setting) {
            case MINIMUM_LIGHT -> rules.set(MINIMUM_LIGHT, Math.clamp(value, 1, 15), server);
            case AFTERGLOW_TICKS -> rules.set(AFTERGLOW_TICKS, Math.clamp(value, 0, 200), server);
            case HEARTBEAT_PERIOD_TICKS -> {
                int period = Math.clamp(value, 0, 1200);
                rules.set(HEARTBEAT_PERIOD_TICKS, period, server);
                if (period > 0 && rules.get(HEARTBEAT_DARK_TICKS) > period) rules.set(HEARTBEAT_DARK_TICKS, period, server);
            }
            case HEARTBEAT_DARK_TICKS -> {
                int period = heartbeatPeriodTicks(level);
                rules.set(HEARTBEAT_DARK_TICKS, Math.clamp(value, 1, period > 0 ? Math.min(200, period) : 200), server);
            }
            case DARK_SWIMMING -> rules.set(DARK_SWIMMING, value != 0, server);
            case VOID_LOOP -> rules.set(VOID_LOOP, value != 0, server);
            case MINE_UNLIT -> rules.set(MINE_UNLIT, value != 0, server);
            case REFRESH -> throw new AssertionError("Refresh handled above");
        }
        // Also acknowledge a no-op edit: controls always settle on the authoritative value.
        sendSettings(player);
        return true;
    }

    public enum Setting {
        MINIMUM_LIGHT, AFTERGLOW_TICKS, HEARTBEAT_PERIOD_TICKS, HEARTBEAT_DARK_TICKS,
        DARK_SWIMMING, VOID_LOOP, REFRESH, MINE_UNLIT
    }

    public record SettingsPayload(int minimumLight, int afterglowTicks, int heartbeatPeriodTicks,
                                  int heartbeatDarkTicks, boolean darkSwimming, boolean voidLoop,
                                  boolean mineUnlit) implements CustomPacketPayload {
        public SettingsPayload {
            minimumLight = Math.clamp(minimumLight, 1, 15);
            afterglowTicks = Math.clamp(afterglowTicks, 0, 200);
            heartbeatPeriodTicks = Math.clamp(heartbeatPeriodTicks, 0, 1200);
            heartbeatDarkTicks = Math.clamp(heartbeatDarkTicks, 1,
                    heartbeatPeriodTicks > 0 ? Math.min(200, heartbeatPeriodTicks) : 200);
        }

        public SettingsPayload(int minimumLight) {
            this(minimumLight, 48, 0, 10, false, false, false);
        }

        public static final Type<SettingsPayload> TYPE = new Type<>(Identifier.fromNamespaceAndPath("lightbound", "settings"));
        public static final StreamCodec<RegistryFriendlyByteBuf, SettingsPayload> CODEC = new StreamCodec<>() {
            @Override public SettingsPayload decode(RegistryFriendlyByteBuf buffer) {
                return new SettingsPayload(buffer.readVarInt(), buffer.readVarInt(), buffer.readVarInt(),
                        buffer.readVarInt(), buffer.readBoolean(), buffer.readBoolean(), buffer.readBoolean());
            }
            @Override public void encode(RegistryFriendlyByteBuf buffer, SettingsPayload settings) {
                buffer.writeVarInt(settings.minimumLight);
                buffer.writeVarInt(settings.afterglowTicks);
                buffer.writeVarInt(settings.heartbeatPeriodTicks);
                buffer.writeVarInt(settings.heartbeatDarkTicks);
                buffer.writeBoolean(settings.darkSwimming);
                buffer.writeBoolean(settings.voidLoop);
                buffer.writeBoolean(settings.mineUnlit);
            }
        };
        @Override public Type<? extends CustomPacketPayload> type() { return TYPE; }
    }

    public record SettingsRequestPayload(int settingId, int value) implements CustomPacketPayload {
        public SettingsRequestPayload(Setting setting, int value) { this(setting.ordinal(), value); }
        public static final Type<SettingsRequestPayload> TYPE = new Type<>(Identifier.fromNamespaceAndPath("lightbound", "settings_request"));
        public static final StreamCodec<RegistryFriendlyByteBuf, SettingsRequestPayload> CODEC = new StreamCodec<>() {
            @Override public SettingsRequestPayload decode(RegistryFriendlyByteBuf buffer) {
                return new SettingsRequestPayload(buffer.readVarInt(), buffer.readVarInt());
            }
            @Override public void encode(RegistryFriendlyByteBuf buffer, SettingsRequestPayload request) {
                buffer.writeVarInt(request.settingId);
                buffer.writeVarInt(request.value);
            }
        };
        @Override public Type<? extends CustomPacketPayload> type() { return TYPE; }
    }
}
