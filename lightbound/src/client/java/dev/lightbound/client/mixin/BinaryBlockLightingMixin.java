package dev.lightbound.client.mixin;

import com.mojang.blaze3d.vertex.QuadInstance;
import dev.lightbound.client.BuiltinDynamicLighting;
import dev.lightbound.client.LichenLighting;
import net.minecraft.client.renderer.block.BlockAndTintGetter;
import net.minecraft.client.renderer.block.BlockQuadOutput;
import net.minecraft.client.renderer.block.ModelBlockRenderer;
import net.minecraft.client.renderer.block.dispatch.BlockStateModel;
import net.minecraft.client.resources.model.geometry.BakedQuad;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** One sample per block, reused by every face, rather than blended corner thresholds. */
@Mixin(ModelBlockRenderer.class)
public abstract class BinaryBlockLightingMixin {
    @Shadow @Final private QuadInstance quadInstance;
    @Unique private int lightbound$blockLight = -1;

    @Inject(method = "tesselateBlock", at = @At("HEAD"))
    private void lightbound$sampleBlock(BlockQuadOutput output, float x, float y, float z,
            BlockAndTintGetter level, BlockPos pos, BlockState state, BlockStateModel model,
            long seed, CallbackInfo ci) {
        lightbound$blockLight = BuiltinDynamicLighting.blockLightCoords(level, state, pos);
    }

    @Inject(method = "putQuadWithTint", at = @At("HEAD"))
    private void lightbound$uniformQuad(BlockQuadOutput output, float x, float y, float z,
            BlockAndTintGetter level, BlockState state, BlockPos pos, BakedQuad quad, CallbackInfo ci) {
        if (lightbound$blockLight < 0) return;
        if (LichenLighting.usesVertexLight(lightbound$blockLight)) {
            for (int i = 0; i < 4; i++)
                quadInstance.setLightCoords(i, LichenLighting.vertexLight(lightbound$blockLight, quadInstance.getLightCoords(i)));
        } else quadInstance.setLightCoords(lightbound$blockLight);
    }
}
