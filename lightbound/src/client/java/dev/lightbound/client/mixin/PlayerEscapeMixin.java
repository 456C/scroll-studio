package dev.lightbound.client.mixin;

import dev.lightbound.LightRules;
import net.minecraft.client.player.LocalPlayer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(LocalPlayer.class)
abstract class PlayerEscapeMixin {
    @Inject(method = "moveTowardsClosestSpace", at = @At("HEAD"), cancellable = true)
    private void lightbound$doNotPushOutOfPhasedTerrain(double x, double z, CallbackInfo ci) {
        if (LightRules.isEscapingTerrain((LocalPlayer)(Object)this)) ci.cancel();
    }
}
