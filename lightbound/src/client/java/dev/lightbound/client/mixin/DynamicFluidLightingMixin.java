package dev.lightbound.client.mixin;

import dev.lightbound.client.BuiltinDynamicLighting;
import dev.lightbound.client.LichenLighting;
import net.minecraft.client.renderer.block.BlockAndTintGetter;
import net.minecraft.client.renderer.block.FluidRenderer;
import net.minecraft.core.BlockPos;
import net.minecraft.util.LightCoordsUtil;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.FluidState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(FluidRenderer.class)
public abstract class DynamicFluidLightingMixin {
    // The renderer can be shared by terrain workers. Keep each tessellation's
    // source-cell sample local to that worker, with no allocation per fluid cell.
    @Unique private final ThreadLocal<int[]> lightbound$fluidLight = ThreadLocal.withInitial(() -> new int[]{-1});

    @Inject(method = "tesselate", at = @At("HEAD"))
    private void lightbound$sampleFluid(BlockAndTintGetter level, BlockPos pos,
            FluidRenderer.Output output, BlockState state, FluidState fluid, CallbackInfo ci) {
        lightbound$fluidLight.get()[0] = BuiltinDynamicLighting.blockLightCoords(level, state, pos);
    }

    @Inject(method = "getLightCoords", at = @At("RETURN"), cancellable = true)
    private void lightbound$movingFluidLight(BlockAndTintGetter level, BlockPos pos, CallbackInfoReturnable<Integer> cir) {
        int wholeFluid = lightbound$fluidLight.get()[0];
        // Vanilla asks for pos.below() on the bottom face. Every binary face must
        // instead use the fluid's own cell, the same position as fluid collision.
        if (wholeFluid >= 0) cir.setReturnValue(LichenLighting.vertexLight(wholeFluid, cir.getReturnValueI()));
    }

    @Redirect(method = "getLightCoords", at = @At(value = "INVOKE", target =
            "Lnet/minecraft/util/LightCoordsUtil;max(II)I"))
    private int lightbound$preserveFractionalFluidLight(int first, int second) {
        // The shared LightCoordsUtil hook already applies moving light and wisp drain
        // to both samples. Keep their fractional precision without applying either again.
        return LightCoordsUtil.smoothPack(Math.max(LightCoordsUtil.smoothBlock(first), LightCoordsUtil.smoothBlock(second)),
                Math.max(LightCoordsUtil.smoothSky(first), LightCoordsUtil.smoothSky(second)));
    }
}
