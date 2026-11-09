package dev.lightbound.client;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import dev.lightbound.Lightbound;
import java.io.IOException;
import java.io.Reader;
import java.io.Writer;
import java.nio.file.AtomicMoveNotSupportedException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import net.fabricmc.loader.api.FabricLoader;

/** Local appearance only. Physical solidity always uses the server's light rules. */
public final class ClientVisualSettings {
    public static volatile boolean dynamicLighting = true;
    public static volatile boolean binaryLighting = true;
    public static volatile boolean edgeGlow = true;
    public static volatile boolean auraParticles = true;
    public static volatile boolean gatewayVoid = false;
    public static volatile boolean curvedEdges = false;
    public static volatile int glowColor = 0xFFB52E;
    public static volatile double litBrightness = 0.85;
    public static volatile double glowStrength = 1.0;
    public static volatile double particleDensity = 0.5;
    public static volatile long revision;

    private static final Gson GSON = new GsonBuilder().setPrettyPrinting().create();
    private static final Path CONFIG = FabricLoader.getInstance().getConfigDir()
            .resolve("lightbound-client.json");
    private static boolean initialized;
    private static boolean dirty;
    private static int saveDelay;

    private ClientVisualSettings() {}

    public static void initialize() {
        if (initialized) return;
        initialized = true;
        reloadFromDisk();
    }

    static void reloadFromDisk() {
        defaults();
        if (Files.isRegularFile(CONFIG)) {
            try (Reader reader = Files.newBufferedReader(CONFIG)) {
                JsonObject json = JsonParser.parseReader(reader).getAsJsonObject();
                dynamicLighting = bool(json, "dynamicLighting", true);
                binaryLighting = bool(json, "binaryLighting", true);
                edgeGlow = bool(json, "edgeGlow", true);
                auraParticles = bool(json, "auraParticles", true);
                gatewayVoid = bool(json, "gatewayVoid", false);
                curvedEdges = bool(json, "curvedEdges", false);
                glowColor = integer(json, "glowColor", 0xFFB52E, 0, 0xFFFFFF);
                litBrightness = number(json, "litBrightness", 0.85, 0.35, 1.0);
                glowStrength = number(json, "glowStrength", 1.0, 0.0, 2.0);
                particleDensity = number(json, "particleDensity", 0.5, 0.0, 1.0);
            } catch (IOException | RuntimeException exception) {
                Lightbound.LOGGER.warn("Could not load Uncommon Nights appearance settings. Using defaults.", exception);
                defaults();
            }
        }
        dirty = false;
        saveDelay = 0;
        revision++;
    }

    /** Call after a live edit; rendering sees the new value immediately. */
    public static void changed() {
        validate();
        revision++;
        dirty = true;
        saveDelay = 20;
    }

    static void tickSave() {
        if (dirty && saveDelay > 0 && --saveDelay == 0) save();
    }

    public static void save() {
        if (!initialized) initialize();
        validate();
        JsonObject json = new JsonObject();
        json.addProperty("dynamicLighting", dynamicLighting);
        json.addProperty("binaryLighting", binaryLighting);
        json.addProperty("edgeGlow", edgeGlow);
        json.addProperty("auraParticles", auraParticles);
        json.addProperty("gatewayVoid", gatewayVoid);
        json.addProperty("curvedEdges", curvedEdges);
        json.addProperty("glowColor", glowColor);
        json.addProperty("litBrightness", litBrightness);
        json.addProperty("glowStrength", glowStrength);
        json.addProperty("particleDensity", particleDensity);
        Path temporary = CONFIG.resolveSibling(CONFIG.getFileName() + ".tmp");
        try {
            Files.createDirectories(CONFIG.getParent());
            try (Writer writer = Files.newBufferedWriter(temporary)) {
                GSON.toJson(json, writer);
            }
            try {
                Files.move(temporary, CONFIG, StandardCopyOption.ATOMIC_MOVE, StandardCopyOption.REPLACE_EXISTING);
            } catch (AtomicMoveNotSupportedException exception) {
                Files.move(temporary, CONFIG, StandardCopyOption.REPLACE_EXISTING);
            }
            dirty = false;
            saveDelay = 0;
        } catch (IOException exception) {
            Lightbound.LOGGER.warn("Could not save Uncommon Nights appearance settings", exception);
            // Avoid writing every frame when a drive is unavailable.
            saveDelay = 200;
        }
    }

    public static void resetDefaults() {
        defaults();
        changed();
    }

    private static void defaults() {
        dynamicLighting = true;
        binaryLighting = true;
        edgeGlow = true;
        auraParticles = true;
        gatewayVoid = false;
        curvedEdges = false;
        glowColor = 0xFFB52E;
        litBrightness = 0.85;
        glowStrength = 1.0;
        particleDensity = 0.5;
    }

    private static void validate() {
        glowColor = Math.clamp(glowColor, 0, 0xFFFFFF);
        litBrightness = finiteClamp(litBrightness, 0.85, 0.35, 1.0);
        glowStrength = finiteClamp(glowStrength, 1.0, 0.0, 2.0);
        particleDensity = finiteClamp(particleDensity, 0.5, 0.0, 1.0);
    }

    private static boolean bool(JsonObject json, String key, boolean fallback) {
        JsonElement value = json.get(key);
        return value != null && value.isJsonPrimitive() && value.getAsJsonPrimitive().isBoolean()
                ? value.getAsBoolean() : fallback;
    }

    private static double number(JsonObject json, String key, double fallback, double min, double max) {
        JsonElement value = json.get(key);
        if (value == null || !value.isJsonPrimitive() || !value.getAsJsonPrimitive().isNumber()) return fallback;
        return finiteClamp(value.getAsDouble(), fallback, min, max);
    }

    private static double finiteClamp(double value, double fallback, double min, double max) {
        return Double.isFinite(value) ? Math.clamp(value, min, max) : fallback;
    }

    private static int integer(JsonObject json, String key, int fallback, int min, int max) {
        JsonElement value = json.get(key);
        if (value == null || !value.isJsonPrimitive() || !value.getAsJsonPrimitive().isNumber()) return fallback;
        double number = value.getAsDouble();
        return Double.isFinite(number) && number == Math.rint(number)
                ? Math.clamp((int) number, min, max) : fallback;
    }
}
