package dev.lightbound;

import java.util.Optional;
import java.util.List;
import java.util.function.Consumer;
import io.netty.buffer.Unpooled;
import net.fabricmc.fabric.api.gametest.v1.GameTest;
import net.minecraft.core.BlockPos;
import net.minecraft.gametest.framework.GameTestHelper;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.permissions.LevelBasedPermissionSet;
import net.minecraft.server.permissions.Permissions;
import net.minecraft.server.players.NameAndId;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.MoverType;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.CollisionContext;

/** Exercises authoritative settings and actual collision/movement with transformed classes. */
public final class LightMechanicsGameTest {
    private static final BlockPos FLOOR = new BlockPos(3, 1, 3);

    @GameTest(maxTicks = 1000, padding = 12)
    public void darkMiningRuleIsEnforcedAtActualBlockDestruction(GameTestHelper test) {
        inDarkRoom(test, player -> {
            var level = test.getLevel();
            var pos = test.absolutePos(FLOOR);
            boolean previous = LightboundSettings.mineUnlit(level);
            try {
                level.getGameRules().set(LightboundSettings.MINE_UNLIT, false, level.getServer());
                player.setGameMode(GameType.CREATIVE);
                test.assertFalse(player.gameMode.destroyBlock(pos), "Even instant mining cannot bypass the dark-block rule");
                test.assertBlockPresent(Blocks.STONE, FLOOR);
                player.setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH));
                test.assertTrue(player.gameMode.destroyBlock(pos), "Held light permits breaking the real block");
                test.setBlock(FLOOR, Blocks.STONE);
                player.setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY);
                level.getGameRules().set(LightboundSettings.MINE_UNLIT, true, level.getServer());
                test.assertTrue(player.gameMode.destroyBlock(pos), "The host toggle restores dark mining");
            } finally {
                level.getGameRules().set(LightboundSettings.MINE_UNLIT, previous, level.getServer());
            }
        });
    }

    @GameTest
    public void authoritativeHistoryCodecPreservesExpiryAndRejectsOversize(GameTestHelper test) {
        var original = new AfterglowSync.HistoryPayload(test.getLevel().dimension().identifier(), 500, 200,
                List.of(new AfterglowTrail.Sample(-17, -16.25, 80.5, 0.25, 14, 450),
                        new AfterglowTrail.Sample(-18, 1.5, 82, -3.25, 15, 499)));
        var buffer = new RegistryFriendlyByteBuf(Unpooled.buffer(), test.getLevel().registryAccess());
        try {
            AfterglowSync.HistoryPayload.CODEC.encode(buffer, original);
            var decoded = AfterglowSync.HistoryPayload.CODEC.decode(buffer);
            test.assertTrue(original.equals(decoded), "History wire format must preserve dimension, exact position, IDs and expiry");
            buffer.clear();
            buffer.writeIdentifier(test.getLevel().dimension().identifier());
            buffer.writeVarLong(500);
            buffer.writeVarInt(200);
            buffer.writeVarInt(AfterglowTrail.MAX_SOURCES_TOTAL + 1);
            boolean rejected = false;
            try { AfterglowSync.HistoryPayload.CODEC.decode(buffer); }
            catch (IllegalArgumentException expected) { rejected = true; }
            test.assertTrue(rejected, "Oversize history packets must be rejected before allocation");
        } finally {
            buffer.release();
        }
        test.succeed();
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void optionalMechanicsStartDisabled(GameTestHelper test) {
        var settings = LightboundSettings.snapshot(test.getLevel());
        test.assertTrue(settings.afterglowTicks() == 48 && settings.heartbeatPeriodTicks() == 0
                && !settings.darkSwimming() && !settings.voidLoop(), "Opt-in mechanics must preserve original gameplay by default");
        var clamped = new LightboundSettings.SettingsPayload(-20, 9999, 5, 9999, true, true, true);
        test.assertTrue(clamped.minimumLight() == 1 && clamped.afterglowTicks() == 200
                && clamped.heartbeatDarkTicks() == 5, "Incoming settings must clamp limits and blackout duration");
        test.succeed();
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void afterglowSupportsGroundButNeverCancelsHeartbeatBlackout(GameTestHelper test) {
        inDarkRoom(test, player -> {
            var level = test.getLevel();
            var rules = level.getGameRules();
            rules.set(LightboundSettings.AFTERGLOW_TICKS, 40, level.getServer());
            player.setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH));
            LightMechanics.recordSources(level);
            player.setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY);
            test.assertFalse(shapeEmpty(test, player), "Recorded afterglow must support a player after the torch is put away");
            BlockPos floor = test.absolutePos(FLOOR);
            test.assertTrue(Math.abs(LightRules.getEffectiveLight(level, floor)
                    - LightRules.createSampler(level).sample(floor)) < 0.00001, "History outline and collision light must agree");
            test.assertTrue(LightRules.sources(level).stream().anyMatch(source -> source.id() < 0), "Native rendering must receive afterglow sources");
            rules.set(LightboundSettings.HEARTBEAT_PERIOD_TICKS, 1, level.getServer());
            rules.set(LightboundSettings.HEARTBEAT_DARK_TICKS, 1, level.getServer());
            test.assertTrue(shapeEmpty(test, player) && LightRules.sources(level).isEmpty(), "Heartbeat blackout must suppress history as well as held light");
            rules.set(LightboundSettings.HEARTBEAT_PERIOD_TICKS, 0, level.getServer());
            test.assertFalse(shapeEmpty(test, player), "A still-valid light history returns after blackout");
            rules.set(LightboundSettings.AFTERGLOW_TICKS, 0, level.getServer());
            test.assertTrue(shapeEmpty(test, player), "Disabling afterglow removes its support immediately");
        });
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void heartbeatOnlyInterruptsCarriedLight(GameTestHelper test) {
        inDarkRoom(test, player -> {
            var level = test.getLevel();
            player.setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH));
            level.getGameRules().set(LightboundSettings.HEARTBEAT_PERIOD_TICKS, 1, level.getServer());
            test.assertTrue(LightRules.heldLuminance(player) == 0 && shapeEmpty(test, player), "Blackout must remove the held torch footprint");
            test.setBlock(FLOOR, Blocks.GLOWSTONE);
            test.assertFalse(shapeEmpty(test, player), "Placed lights must remain dependable through a heartbeat blackout");
            level.getGameRules().set(LightboundSettings.HEARTBEAT_PERIOD_TICKS, 0, level.getServer());
            test.assertTrue(LightRules.heldLuminance(player) == 14, "Disabling heartbeat restores the original torch emission");
        });
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void darkSwimmingRequiresJumpAndStopsWhenIlluminated(GameTestHelper test) {
        inDarkRoom(test, player -> {
            var level = test.getLevel();
            player.setDeltaMovement(0, -1, 0);
            test.assertFalse(LightMechanics.applyDarkSwimming(player, true), "Default-off swimming must not change movement");
            level.getGameRules().set(LightboundSettings.DARK_SWIMMING, true, level.getServer());
            test.assertFalse(LightMechanics.applyDarkSwimming(player, false), "Swimming requires jump input");
            player.fallDistance = 50;
            double before = player.getY();
            test.assertTrue(LightMechanics.applyDarkSwimming(player, true), "Jump should swim upward through darkness");
            player.move(MoverType.SELF, player.getDeltaMovement());
            test.assertTrue(player.getY() > before && player.fallDistance == 0, "Dark swimming must produce actual upward movement and reset accumulated fall distance");
            player.setItemSlot(EquipmentSlot.OFFHAND, new ItemStack(Items.TORCH));
            test.assertFalse(LightMechanics.applyDarkSwimming(player, true), "Illuminated air must use normal movement");
            player.setItemSlot(EquipmentSlot.OFFHAND, ItemStack.EMPTY);
            player.getAbilities().flying = true;
            test.assertFalse(LightMechanics.applyDarkSwimming(player, true), "Creative flight must retain its own controls");
        });
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void voidLoopReturnsOnlyThePlayerToTheSameColumn(GameTestHelper test) {
        inDarkRoom(test, player -> {
            var level = test.getLevel();
            double x = player.getX();
            double z = player.getZ();
            player.setPos(x, level.getMinY() - 40.0, z);
            player.fallDistance = 300;
            player.setDeltaMovement(0, -3, 0);
            test.assertFalse(LightMechanics.tryVoidLoop(player), "Void loop must remain opt-in");
            level.getGameRules().set(LightboundSettings.VOID_LOOP, true, level.getServer());
            test.assertTrue(LightMechanics.tryVoidLoop(player), "Crossing the void boundary must wrap the player");
            test.assertTrue(player.getX() == x && player.getZ() == z
                    && player.getY() == level.getMaxY() + 16.0, "Void looping must preserve the horizontal column");
            test.assertTrue(player.fallDistance == 0 && player.getDeltaMovement().equals(Vec3.ZERO), "The teleport must clear old falling momentum and distance");
        });
    }

    @GameTest(maxTicks = 1000, padding = 12)
    public void gameplayRequestsRequireGamemasterAndClampValues(GameTestHelper test) {
        inDarkRoom(test, player -> {
            var level = test.getLevel();
            var players = level.getServer().getPlayerList();
            NameAndId profile = new NameAndId(player.getGameProfile());
            try {
                players.op(profile, Optional.of(LevelBasedPermissionSet.MODERATOR), Optional.of(false));
                test.assertFalse(player.permissions().hasPermission(Permissions.COMMANDS_GAMEMASTER), "Fixture must have insufficient permission");
                test.assertFalse(LightboundSettings.applyRequest(player, new LightboundSettings.SettingsRequestPayload(
                        LightboundSettings.Setting.AFTERGLOW_TICKS, 150)), "A modified client cannot bypass server permissions");
                test.assertTrue(LightboundSettings.afterglowTicks(level) == 0, "A rejected request must not change rules");
                players.op(profile, Optional.of(LevelBasedPermissionSet.GAMEMASTER), Optional.of(false));
                test.assertFalse(LightboundSettings.applyRequest(player, new LightboundSettings.SettingsRequestPayload(
                        LightboundSettings.Setting.AFTERGLOW_TICKS, 9999)), "Operator status cannot impersonate the host");
                test.assertTrue(LightboundSettings.afterglowTicks(level) == 0, "A non-host cannot change settings");
                level.getServer().getCommands().performPrefixedCommand(player.createCommandSourceStack(),
                        "gamerule lightbound:afterglow_ticks 150");
                test.assertTrue(LightboundSettings.afterglowTicks(level) == 0,
                        "Ordinary operator gamerule commands must also respect host ownership");
                test.assertFalse(LightboundSettings.applyRequest(player, new LightboundSettings.SettingsRequestPayload(9999, 0)), "Unknown request keys must be ignored");
            } finally {
                players.deop(profile);
            }
        });
    }

    private static boolean shapeEmpty(GameTestHelper test, ServerPlayer player) {
        BlockPos pos = test.absolutePos(FLOOR);
        return test.getLevel().getBlockState(pos).getCollisionShape(test.getLevel(), pos, CollisionContext.of(player)).isEmpty();
    }

    @SuppressWarnings("removal")
    private static void inDarkRoom(GameTestHelper test, Consumer<ServerPlayer> assertions) {
        for (int x = 0; x < 8; x++) for (int y = 0; y < 8; y++) for (int z = 0; z < 8; z++) {
            test.setBlock(x, y, z, x == 0 || x == 7 || y == 0 || y == 7 || z == 0 || z == 7 ? Blocks.STONE : Blocks.AIR);
        }
        test.setBlock(FLOOR, Blocks.STONE);
        test.startSequence().thenWaitUntil(() -> test.assertTrue(LightRules.getEffectiveLight(test.getLevel(), test.absolutePos(FLOOR)) < 1,
                "Waiting for chamber skylight to settle")).thenExecute(() -> {
            var level = test.getLevel();
            var saved = LightboundSettings.snapshot(level);
            ServerPlayer player = test.makeMockServerPlayerInLevel();
            LightMechanics.clearLevel(level);
            try {
                var rules = level.getGameRules();
                rules.set(LightboundSettings.MINIMUM_LIGHT, 1, level.getServer());
                rules.set(LightboundSettings.AFTERGLOW_TICKS, 0, level.getServer());
                rules.set(LightboundSettings.HEARTBEAT_PERIOD_TICKS, 0, level.getServer());
                rules.set(LightboundSettings.HEARTBEAT_DARK_TICKS, 10, level.getServer());
                rules.set(LightboundSettings.DARK_SWIMMING, false, level.getServer());
                rules.set(LightboundSettings.VOID_LOOP, false, level.getServer());
                player.setGameMode(GameType.SURVIVAL);
                player.getAbilities().flying = false;
                BlockPos floor = test.absolutePos(FLOOR);
                player.setPos(floor.getX() + 0.5, floor.getY() + 1, floor.getZ() + 0.5);
                assertions.accept(player);
            } finally {
                LightMechanics.clearLevel(level);
                level.getServer().getPlayerList().remove(player);
                var rules = level.getGameRules();
                rules.set(LightboundSettings.MINIMUM_LIGHT, saved.minimumLight(), level.getServer());
                rules.set(LightboundSettings.AFTERGLOW_TICKS, saved.afterglowTicks(), level.getServer());
                rules.set(LightboundSettings.HEARTBEAT_PERIOD_TICKS, saved.heartbeatPeriodTicks(), level.getServer());
                rules.set(LightboundSettings.HEARTBEAT_DARK_TICKS, saved.heartbeatDarkTicks(), level.getServer());
                rules.set(LightboundSettings.DARK_SWIMMING, saved.darkSwimming(), level.getServer());
                rules.set(LightboundSettings.VOID_LOOP, saved.voidLoop(), level.getServer());
            }
        }).thenSucceed();
    }
}
