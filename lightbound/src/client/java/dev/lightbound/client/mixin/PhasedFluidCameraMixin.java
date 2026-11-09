package dev.lightbound.client.mixin;

import dev.lightbound.LightRules;
import net.minecraft.client.Camera;
import net.minecraft.client.Minecraft;
import net.minecraft.world.level.material.FogType;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(Camera.class)
abstract class PhasedFluidCameraMixin {
    @Inject(method = "getFluidInCamera", at = @At("RETURN"), cancellable = true)
    private void lightbound$noPhasedFluidFog(CallbackInfoReturnable<FogType> cir) {
        var player = Minecraft.getInstance().player;
        if (player != null && (cir.getReturnValue() == FogType.WATER || cir.getReturnValue() == FogType.LAVA)
                && LightRules.phasesFluid(player, ((Camera)(Object)this).blockPosition())) {
            cir.setReturnValue(FogType.NONE);
        }
    }
}
