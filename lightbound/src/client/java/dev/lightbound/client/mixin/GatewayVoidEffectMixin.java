package dev.lightbound.client.mixin;

import com.mojang.blaze3d.resource.CrossFrameResourcePool;
import dev.lightbound.client.GatewayVoidEffect;
import net.minecraft.client.DeltaTracker;
import net.minecraft.client.renderer.GameRenderer;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

@Mixin(GameRenderer.class)
public abstract class GatewayVoidEffectMixin {
    @Shadow @Final private CrossFrameResourcePool resourcePool;

    @Inject(method = "update", at = @At("TAIL"))
    private void lightbound$addGatewayVoid(DeltaTracker deltaTracker, CallbackInfo ci) {
        GatewayVoidEffect.update((GameRenderer) (Object) this);
    }

    @Inject(method = "renderLevel", at = @At(value = "INVOKE",
            target = "Lnet/minecraft/client/renderer/GameRenderer;render3dHud(Lnet/minecraft/client/renderer/state/level/CameraRenderState;Lnet/minecraft/client/renderer/state/level/PlayerRenderState;Lnet/minecraft/client/renderer/state/OptionsRenderState;Z)V"))
    private void lightbound$renderGatewayVoid(CallbackInfo ci) {
        GatewayVoidEffect.render((GameRenderer) (Object) this, resourcePool);
    }
}
