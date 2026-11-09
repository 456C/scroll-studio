package dev.lightbound.client.mixin;

import dev.lightbound.LightboundSettings;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.MultiPlayerGameMode;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Shadow;
import org.spongepowered.asm.mixin.Final;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(MultiPlayerGameMode.class)
public abstract class MiningPredictionMixin {
    @Shadow @Final private Minecraft minecraft;
    @Shadow public abstract void stopDestroyBlock();

    @Inject(method = {"startDestroyBlock", "continueDestroyBlock"}, at = @At("HEAD"), cancellable = true)
    private void lightbound$requireLight(BlockPos pos, Direction direction, CallbackInfoReturnable<Boolean> cir) {
        if (minecraft.level != null && !LightboundSettings.canMine(minecraft.level, pos)) {
            stopDestroyBlock();
            cir.setReturnValue(false);
        }
    }

    @Inject(method = "destroyBlock", at = @At("HEAD"), cancellable = true)
    private void lightbound$guardInstantBreak(BlockPos pos, CallbackInfoReturnable<Boolean> cir) {
        if (minecraft.level != null && !LightboundSettings.canMine(minecraft.level, pos)) cir.setReturnValue(false);
    }
}
