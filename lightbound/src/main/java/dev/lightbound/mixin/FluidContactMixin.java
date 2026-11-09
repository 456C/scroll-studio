package dev.lightbound.mixin;

import dev.lightbound.LightRules;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.phys.Vec3;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(Entity.class)
abstract class FluidContactMixin {
    @Inject(method = "collidedWithFluid", at = @At("HEAD"), cancellable = true)
    private void lightbound$ignoreDarkContact(FluidState fluid, BlockPos pos, Vec3 from, Vec3 to,
            CallbackInfoReturnable<Boolean> cir) {
        if (LightRules.phasesFluid((Entity)(Object)this, pos)) cir.setReturnValue(false);
    }
}
