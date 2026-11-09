package dev.lightbound;

/** Shared server/client light math; values use Minecraft's 0–15 light scale. */
public final class LightMath {
    public static final double DYNAMIC_RADIUS = 7.75;
    public static final double SOLID_LIGHT_THRESHOLD = 1.0;

    private LightMath() {}

    /** Vanilla leaves four levels of moonlight at night; Lightbound removes it. */
    public static double daylightFactor(int skyDarken) {
        return Math.clamp((11.0 - skyDarken) / 11.0, 0.0, 1.0);
    }

    /** Matches LambDynamicLights' linear attenuation, including dimmer emitters. */
    public static double dynamicLight(int luminance, double distance) {
        return Math.max(0.0, Math.clamp(luminance, 0, 15)
                - Math.max(0.0, distance) * 15.0 / DYNAMIC_RADIUS);
    }

    public static boolean isSolid(double light) {
        return isSolid(light, SOLID_LIGHT_THRESHOLD);
    }

    public static boolean isSolid(double light, double threshold) {
        return light >= threshold;
    }

    /** The end of each shared world-time cycle is the short blackout. Zero disables it. */
    public static boolean heartbeatDark(long gameTime, int periodTicks, int darkTicks) {
        if (periodTicks <= 0) return false;
        int darkness = Math.clamp(darkTicks, 1, periodTicks);
        return Math.floorMod(gameTime, periodTicks) >= periodTicks - darkness;
    }

    public static boolean afterglowActive(long capturedTick, long gameTime, int durationTicks) {
        long age = gameTime - capturedTick;
        return durationTicks > 0 && age >= 0 && age < durationTicks;
    }
}
