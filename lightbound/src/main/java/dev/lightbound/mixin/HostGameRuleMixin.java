package dev.lightbound.mixin;

import com.mojang.brigadier.context.CommandContext;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.network.chat.Component;
import net.minecraft.server.commands.GameRuleCommand;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.gamerules.GameRule;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Player command edits follow the same host rule as the menu; server console stays available. */
@Mixin(GameRuleCommand.class)
public abstract class HostGameRuleMixin {
    @Inject(method = "setRule", at = @At("HEAD"), cancellable = true)
    private static <T> void lightbound$hostOnly(CommandContext<CommandSourceStack> context,
            GameRule<T> rule, CallbackInfoReturnable<Integer> cir) {
        var source = context.getSource();
        if (rule.getIdentifier().getNamespace().equals("lightbound")
                && source.getEntity() instanceof ServerPlayer player
                && !source.getServer().isSingleplayerOwner(player.nameAndId())) {
            source.sendFailure(Component.literal("Only the world host or server console can change Uncommon Nights gameplay settings."));
            cir.setReturnValue(0);
        }
    }
}
