package dev.lightbound;

import net.fabricmc.api.ModInitializer;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

public final class Lightbound implements ModInitializer {
    public static final Logger LOGGER = LoggerFactory.getLogger("lightbound");

    @Override
    public void onInitialize() {
        LightboundSettings.initialize();
        LightMechanics.initialize();
        LOGGER.info("Uncommon Nights initialized.");
    }
}
