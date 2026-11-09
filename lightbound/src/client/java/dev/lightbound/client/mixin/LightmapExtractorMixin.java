package dev.lightbound.client.mixin;

import dev.lightbound.LightRules;
import dev.lightbound.client.ClientVisualSettings;
import dev.lightbound.LightboundSettings;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.LightmapRenderStateExtractor;
import net.minecraft.client.renderer.state.LightmapRenderState;
import org.joml.Vector3f;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Keeps the rendered lightmap on the same daylight scale as physical solidity. */
@Mixin(LightmapRenderStateExtractor.class)
public abstract class LightmapExtractorMixin {
    @Inject(method = "extract", at = @At("TAIL"))
    private void lightbound$extractDarkness(LightmapRenderState state, float partialTicks, CallbackInfo ci) {
        var level = Minecraft.getInstance().level;
        if (level == null || !LightboundSettings.isActive(level)) {
            return;
        }
        state.skyFactor = (float) LightRules.daylightFactor(level);
        // A negative ambient color marks our shader branch without changing the UBO layout.
        // The shader uses zero ambient in this branch. Potions still improve vision only.
        state.ambientColor = new Vector3f(-1.0f,
                ClientVisualSettings.binaryLighting ? LightboundSettings.threshold(level) : 0.0f,
                (float)ClientVisualSettings.litBrightness);
    }
}
