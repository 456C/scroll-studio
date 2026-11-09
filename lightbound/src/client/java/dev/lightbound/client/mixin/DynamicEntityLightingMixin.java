package dev.lightbound.client.mixin;

import dev.lightbound.client.BuiltinDynamicLighting;
import net.minecraft.client.renderer.entity.EntityRenderer;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Mobs and items receive visual light only; their physics are unchanged. */
@Mixin(EntityRenderer.class)
public abstract class DynamicEntityLightingMixin {
    @Inject(method = "getPackedLightCoords", at = @At("RETURN"), cancellable = true)
    private void lightbound$movingEntityLight(Entity entity, float partialTicks, CallbackInfoReturnable<Integer> cir) {
        cir.setReturnValue(BuiltinDynamicLighting.lightCoords(entity.level(),
                BlockPos.containing(entity.getLightProbePosition(partialTicks)), cir.getReturnValueI()));
    }
}
