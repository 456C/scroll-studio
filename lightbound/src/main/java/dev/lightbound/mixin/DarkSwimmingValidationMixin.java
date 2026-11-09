package dev.lightbound.mixin;

import dev.lightbound.LightMechanics;
import net.minecraft.network.protocol.game.ServerboundMovePlayerPacket;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.network.ServerGamePacketListenerImpl;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** The optional server rule explicitly authorizes rising while swimming in darkness. */
@Mixin(ServerGamePacketListenerImpl.class)
public abstract class DarkSwimmingValidationMixin {
    @Shadow public ServerPlayer player;
    @Shadow private boolean clientIsFloating;

    @Inject(method = "handleMovePlayer", at = @At("TAIL"))
    private void lightbound$allowDarkSwimming(ServerboundMovePlayerPacket packet, CallbackInfo ci) {
        if (LightMechanics.darkSwimmingActive(player)) clientIsFloating = false;
    }

    @Inject(method = "tick", at = @At("HEAD"))
    private void lightbound$clearAuthorizedFloating(CallbackInfo ci) {
        if (LightMechanics.darkSwimmingActive(player)) clientIsFloating = false;
    }
}
