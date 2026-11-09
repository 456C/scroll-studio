package dev.lightbound.mixin;

import dev.lightbound.LightRules;
import dev.lightbound.LightboundSettings;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.BooleanOp;
import net.minecraft.world.phys.shapes.Shapes;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(Entity.class)
abstract class EntitySuffocationMixin {
    @Inject(method = "isInWall", at = @At("HEAD"), cancellable = true)
    private void lightbound$ignorePhasedWalls(CallbackInfoReturnable<Boolean> cir) {
        Entity entity = (Entity) (Object) this;
        if (!LightRules.appliesTo(entity) || !LightboundSettings.isActive(entity.level())) {
            return;
        }
        Level level = entity.level();
        double width = entity.getBbWidth() * 0.8;
        AABB eyes = AABB.ofSize(entity.getEyePosition(), width, 1.0e-6, width);
        boolean suffocating = BlockPos.betweenClosedStream(eyes).anyMatch(pos -> {
            BlockState state = level.getBlockState(pos);
            return !state.isAir() && state.isSuffocating(level, pos)
                    && !LightRules.isPhased(state, level, pos, entity)
                    && Shapes.joinIsNotEmpty(state.getCollisionShape(level, pos).move(pos),
                            Shapes.create(eyes), BooleanOp.AND);
        });
        cir.setReturnValue(suffocating);
    }
}
