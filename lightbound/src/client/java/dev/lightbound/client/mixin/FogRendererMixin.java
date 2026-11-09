package dev.lightbound.client.mixin;

import dev.lightbound.LightRules;
import dev.lightbound.LightboundSettings;
import net.minecraft.client.Camera;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.renderer.fog.FogRenderer;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.material.FogType;
import org.joml.Vector4f;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Prevents the atmospheric fog color from revealing otherwise black terrain. */
@Mixin(FogRenderer.class)
public abstract class FogRendererMixin {
    @Inject(method = "computeFogColor", at = @At("TAIL"))
    private void lightbound$darkAtmosphere(Camera camera, float partialTicks, ClientLevel level,
            int renderDistance, float worldDarkening, Vector4f color, CallbackInfo ci) {
        // Lava remains visibly emissive. This changes air and water atmosphere only.
        if (!LightboundSettings.isActive(level) || camera.getFluidInCamera() == FogType.LAVA) {
            return;
        }
        float skyExposure = level.getBrightness(LightLayer.SKY, camera.blockPosition()) / 15.0f;
        float visibility = skyExposure * (float) LightRules.daylightFactor(level);
        color.x *= visibility;
        color.y *= visibility;
        color.z *= visibility;
    }
}
