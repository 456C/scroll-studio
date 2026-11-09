package dev.lightbound.mixin;

import dev.lightbound.LightRules;
import dev.lightbound.LightboundSettings;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.EntityCollisionContext;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Vanilla's position context shares the placement flag, but validates MOVEMENT. */
@Mixin(CollisionContext.class)
public interface MovementContextMixin {
    @Inject(method = "withPosition", at = @At("HEAD"), cancellable = true)
    private static void lightbound$validatePhasedMovement(Entity entity, double oldY,
            CallbackInfoReturnable<CollisionContext> cir) {
        if (LightRules.appliesTo(entity) && LightboundSettings.isActive(entity.level())) {
            cir.setReturnValue(new MovementContext((Player)entity, oldY));
        }
    }

    final class MovementContext extends EntityCollisionContext {
        public MovementContext(Player player, double oldY) {
            super(player.isDescending(), false, oldY, player.getMainHandItem(), false, player);
        }
    }
}
