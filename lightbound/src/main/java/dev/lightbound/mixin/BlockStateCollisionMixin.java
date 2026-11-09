package dev.lightbound.mixin;

import dev.lightbound.LightRules;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.EntityCollisionContext;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(BlockBehaviour.BlockStateBase.class)
abstract class BlockStateCollisionMixin {
    @Inject(method = "getEntityInsideCollisionShape", at = @At("HEAD"), cancellable = true)
    private void lightbound$ignoreDarkInsideEffects(BlockGetter world, BlockPos pos,
            net.minecraft.world.entity.Entity entity, CallbackInfoReturnable<VoxelShape> cir) {
        if (LightRules.isPhased((BlockState)(Object)this, world, pos, entity)) cir.setReturnValue(Shapes.empty());
    }

    @Inject(method = "getCollisionShape(Lnet/minecraft/world/level/BlockGetter;Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/phys/shapes/CollisionContext;)Lnet/minecraft/world/phys/shapes/VoxelShape;",
            at = @At("HEAD"), cancellable = true)
    private void lightbound$phaseForPlayers(BlockGetter world, BlockPos pos,
            CollisionContext context, CallbackInfoReturnable<VoxelShape> cir) {
        if (!context.isPlacement() && context instanceof EntityCollisionContext entityContext
                && LightRules.isPhased((BlockState) (Object) this, world, pos, entityContext.getEntity())) {
            cir.setReturnValue(Shapes.empty());
        }
    }
}
