package dev.lightbound.client.mixin;

import dev.lightbound.client.BuiltinDynamicLighting;
import net.minecraft.core.BlockPos;
import net.minecraft.util.LightCoordsUtil;
import net.minecraft.world.level.BlockAndLightGetter;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Shared native light-coordinate path for terrain, block entities, and particles. */
@Mixin(LightCoordsUtil.class)
public abstract class DynamicLightCoordsMixin {
    @Inject(method = "getLightCoords(Lnet/minecraft/util/LightCoordsUtil$BrightnessGetter;Lnet/minecraft/world/level/BlockAndLightGetter;Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/core/BlockPos;)I",
            at = @At("RETURN"), cancellable = true)
    private static void lightbound$movingLight(LightCoordsUtil.BrightnessGetter brightnessGetter,
            BlockAndLightGetter level, BlockState state, BlockPos pos, CallbackInfoReturnable<Integer> cir) {
        cir.setReturnValue(BuiltinDynamicLighting.lightCoords(level, pos, cir.getReturnValueI()));
    }
}
