package dev.lightbound.mixin;

import dev.lightbound.LightRules;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityFluidInteraction;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.level.material.Fluids;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

@Mixin(EntityFluidInteraction.class)
abstract class FluidInteractionMixin {
    @Redirect(method = "update", at = @At(value = "INVOKE", target =
            "Lnet/minecraft/world/level/BlockGetter;getFluidState(Lnet/minecraft/core/BlockPos;)Lnet/minecraft/world/level/material/FluidState;"))
    private FluidState lightbound$ignoreDarkFluid(BlockGetter level, BlockPos pos, Entity entity, boolean ignoreCurrent) {
        FluidState fluid = level.getFluidState(pos);
        return !fluid.isEmpty() && LightRules.phasesFluid(entity, pos) ? Fluids.EMPTY.defaultFluidState() : fluid;
    }
}
