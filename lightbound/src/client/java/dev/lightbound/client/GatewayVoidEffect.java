package dev.lightbound.client;

import com.mojang.blaze3d.resource.GraphicsResourceAllocator;
import dev.lightbound.LightboundSettings;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientLifecycleEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.GameRenderer;
import net.minecraft.client.renderer.PostChain;
import net.minecraft.client.renderer.texture.SimpleTexture;
import net.minecraft.resources.Identifier;
import java.util.Set;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/** Optional single world post-effect, using Minecraft's own portal texture resources. */
public final class GatewayVoidEffect {
    public static final Identifier EFFECT = Identifier.fromNamespaceAndPath("lightbound", "gateway_void");
    private static final Identifier PORTAL_ALIAS = Identifier.fromNamespaceAndPath("lightbound", "textures/effect/end_portal.png");
    private static final Identifier SKY_ALIAS = Identifier.fromNamespaceAndPath("lightbound", "textures/effect/end_sky.png");
    private static final Set<Identifier> TARGETS = Set.of(PostChain.MAIN_TARGET_ID);
    private static final Logger LOGGER = LoggerFactory.getLogger("lightbound/gateway-void");
    private static volatile boolean enabled;
    private static volatile boolean buried;
    private static final Identifier OCCLUDED = Identifier.fromNamespaceAndPath("lightbound", "occluded");
    private static volatile boolean failed;

    private GatewayVoidEffect() {}

    public static void initialize() {
        ClientLifecycleEvents.CLIENT_STARTED.register(client -> {
            // Post-chain JSON resolves textures inside textures/effect. Each alias owns
            // its texture, but reads the original vanilla resource, including pack overrides.
            var textures = client.getTextureManager();
            textures.registerAndLoad(PORTAL_ALIAS, new SimpleTexture(
                    Identifier.withDefaultNamespace("textures/entity/end_portal/end_portal.png")));
            textures.registerAndLoad(SKY_ALIAS, new SimpleTexture(
                    Identifier.withDefaultNamespace("textures/environment/end_sky.png")));
        });
    }

    public static void update(GameRenderer renderer) {
        Minecraft client = Minecraft.getInstance();
        buried = client.level != null && client.player != null && !client.player.isSpectator()
                && client.level.getBlockState(net.minecraft.core.BlockPos.containing(client.player.getEyePosition()))
                    .isSolidRender();
        boolean next = (buried || ClientVisualSettings.gatewayVoid) && client.level != null && LightboundSettings.isActive(client.level);
        if (next != enabled) failed = false;
        enabled = next;
    }

    public static void render(GameRenderer renderer, GraphicsResourceAllocator resources) {
        if (!enabled || failed) return;
        try {
            // This runs after the world and before the hand, screen effects, or GUI.
            // ShaderManager owns the cached pipeline; vanilla's pool reuses its target.
            PostChain chain = Minecraft.getInstance().getShaderManager().getPostChain(buried ? OCCLUDED : EFFECT, TARGETS);
            if (chain != null) chain.process(renderer.mainRenderTarget(), resources);
        } catch (RuntimeException exception) {
            failed = true;
            LOGGER.warn("Could not render the experimental gateway void; toggle the option to retry", exception);
        }
    }
}
