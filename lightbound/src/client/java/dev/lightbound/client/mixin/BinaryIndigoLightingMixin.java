package dev.lightbound.client.mixin;

import dev.lightbound.client.BuiltinDynamicLighting;
import dev.lightbound.client.LichenLighting;
import net.fabricmc.fabric.api.client.renderer.v1.mesh.MutableQuadView;
import net.fabricmc.fabric.api.client.renderer.v1.mesh.QuadEmitter;
import net.minecraft.client.renderer.block.BlockAndTintGetter;
import net.minecraft.client.renderer.block.dispatch.BlockStateModel;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Fabric API replaces the vanilla terrain renderer, including for ordinary vanilla blocks. */
@Mixin(targets = "net.fabricmc.fabric.impl.client.indigo.renderer.render.AltModelBlockRendererImpl", remap = false)
public abstract class BinaryIndigoLightingMixin {
    @Unique private int lightbound$blockLight = -1;

    @Inject(method = "tesselateBlock", at = @At("HEAD"))
    private void lightbound$sampleBlock(QuadEmitter output, float x, float y, float z,
            BlockAndTintGetter level, BlockPos pos, BlockState state, BlockStateModel model,
            long seed, CallbackInfo ci) {
        lightbound$blockLight = BuiltinDynamicLighting.blockLightCoords(level, state, pos);
    }

    @Inject(method = "transform", at = @At("RETURN"))
    private void lightbound$uniformQuad(MutableQuadView quad, CallbackInfoReturnable<Boolean> cir) {
        // Keep texture tint, face shading, and AO; replace only attenuated lightmap coordinates.
        if (cir.getReturnValueZ() && lightbound$blockLight >= 0 && !quad.emissive()) {
            if (LichenLighting.usesVertexLight(lightbound$blockLight)) {
                for (int i = 0; i < 4; i++)
                    quad.lightmap(i, LichenLighting.vertexLight(lightbound$blockLight, quad.lightmap(i)));
            } else {
                quad.lightmap(lightbound$blockLight, lightbound$blockLight, lightbound$blockLight, lightbound$blockLight);
            }
        }
    }
}
