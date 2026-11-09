package dev.lightbound.client.mixin;

import dev.lightbound.LightRules;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.client.renderer.extract.LevelExtractor;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Redirect;

/** A phased wall must not put vanilla's opaque suffocation texture over the view. */
@Mixin(LevelExtractor.class)
public abstract class LevelExtractorMixin {
    @Redirect(method = "getViewBlockingState", at = @At(value = "INVOKE", target =
            "Lnet/minecraft/world/level/block/state/BlockState;isViewBlocking(Lnet/minecraft/world/level/BlockGetter;Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/phys/AABB;)Z"))
    private static boolean lightbound$ignorePhasedOverlay(BlockState state, BlockGetter world,
            BlockPos pos, AABB nearPlane, LocalPlayer player, Frustum frustum) {
        return !LightRules.isPhased(state, world, pos, player)
                && state.isViewBlocking(world, pos, nearPlane);
    }
}
