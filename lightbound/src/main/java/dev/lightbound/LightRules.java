package dev.lightbound;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.SectionPos;
import net.minecraft.world.attribute.EnvironmentAttributes;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.BooleanOp;
import net.minecraft.world.phys.shapes.Shapes;

/** Changes player collision only. Blocks, light propagation and world saves stay intact. */
public final class LightRules {
    private static final Direction[] FACES = Direction.values();
    private static final EquipmentSlot[] EQUIPMENT = EquipmentSlot.values();

    private LightRules() {}

    public static boolean appliesTo(Entity entity) {
        return entity instanceof Player player
                && !player.isSpectator()
                && !player.getAbilities().flying;
    }

    public static boolean isPhased(BlockState state, BlockGetter world, BlockPos pos, Entity entity) {
        if (!appliesTo(entity) || !(world instanceof Level level) || !LightboundSettings.isActive(level)
                || state.isAir()) {
            return false;
        }

        // A torch switched on inside terrain must not entomb its carrier. Once the
        // player leaves this block it immediately regains normal collision.
        if (isEscapingTerrain(entity)) return true;
        return !LightMath.isSolid(getEffectiveLight(level, pos), LightboundSettings.threshold(level));
    }

    public static boolean isAlwaysSolid(BlockState state) {
        return false;
    }

    /** An exit must span the whole body, including the next block in the movement sweep. */
    public static boolean isEscapingTerrain(Entity entity) {
        if (!appliesTo(entity) || !LightboundSettings.isActive(entity.level())) return false;
        AABB body = entity.getBoundingBox().deflate(1.0e-5);
        var shape = Shapes.create(body);
        for (BlockPos occupied : BlockPos.betweenClosed(body)) {
            var original = entity.level().getBlockState(occupied).getCollisionShape(entity.level(), occupied);
            if (!original.isEmpty() && Shapes.joinIsNotEmpty(original.move(occupied), shape, BooleanOp.AND)) {
                return true;
            }
        }
        return false;
    }

    public static boolean phasesFluid(Entity entity, BlockPos pos) {
        return appliesTo(entity) && LightboundSettings.isActive(entity.level())
                && !LightMath.isSolid(getEffectiveLight(entity.level(), pos), LightboundSettings.threshold(entity.level()));
    }

    /** Opaque blocks store zero light internally: sample light on their faces. */
    public static double getEffectiveLight(Level level, BlockPos pos) {
        double light = worldLight(level, pos, daylightFactor(level));
        if (light >= 15.0) {
            return light;
        }

        double x = pos.getX() + 0.5;
        double y = pos.getY() + 0.5;
        double z = pos.getZ() + 0.5;
        double radiusSquared = LightMath.DYNAMIC_RADIUS * LightMath.DYNAMIC_RADIUS;
        for (Player player : level.players()) {
            if (player.isSpectator()) {
                continue;
            }
            double dx = player.getX() - x;
            double dy = player.getEyeY() - y;
            double dz = player.getZ() - z;
            double distanceSquared = dx * dx + dy * dy + dz * dz;
            if (distanceSquared >= radiusSquared) {
                continue;
            }
            int luminance = heldLuminance(player);
            if (luminance > 0) {
                light = Math.max(light, LightMath.dynamicLight(luminance, Math.sqrt(distanceSquared)));
            }
        }
        light = Math.max(light, LightMechanics.afterglowLight(level, pos));
        return light;
    }

    /** Capture moving emitters once for an overlay scan, without caching gameplay collision. */
    public static LightSampler createSampler(Level level) {
        return new LightSampler(level);
    }

    /** Shared current/history snapshot for outlines and native renderer workers. */
    public static List<LightSource> sources(Level level) {
        List<LightSource> sources = new ArrayList<>();
        for (Player player : level.players()) {
            if (player.isSpectator()) continue;
            int luminance = heldLuminance(player);
            if (luminance > 0) sources.add(new LightSource(player.getId(), player.getX(), player.getEyeY(), player.getZ(), luminance));
        }
        sources.addAll(LightMechanics.afterglowSources(level));
        return List.copyOf(sources);
    }

    public static boolean heartbeatDark(Level level) {
        return LightMath.heartbeatDark(level.getGameTime(), LightboundSettings.heartbeatPeriodTicks(level),
                LightboundSettings.heartbeatDarkTicks(level));
    }

    /** The End stores skylight in 26.3, but its dimension explicitly supplies no sky illumination. */
    public static double daylightFactor(Level level) {
        float dimensionFactor = level.dimensionType().attributes()
                .applyModifier(EnvironmentAttributes.SKY_LIGHT_FACTOR, 1.0f);
        return Math.clamp(dimensionFactor, 0.0f, 1.0f) * LightMath.daylightFactor(level.getSkyDarken());
    }

    private static double worldLight(Level level, BlockPos pos, double daylight) {
        double light = Math.max(level.getBlockState(pos).getLightEmission(),
                sampleWorldLight(level, pos, daylight));
        BlockPos.MutableBlockPos face = new BlockPos.MutableBlockPos();
        for (Direction direction : FACES) {
            face.setWithOffset(pos, direction);
            light = Math.max(light, sampleWorldLight(level, face, daylight));
        }
        return light;
    }

    private static double sampleWorldLight(Level level, BlockPos pos, double daylight) {
        return Math.max(level.getBrightness(LightLayer.BLOCK, pos),
                level.getBrightness(LightLayer.SKY, pos) * daylight);
    }

    /** Both hands/equipment count, as in the default LambDynamicLights provider. */
    public static int heldLuminance(Player player) {
        if (heartbeatDark(player.level())) return 0;
        int luminance = player.isOnFire() ? 15 : 0;
        boolean submerged = !player.level().getFluidState(BlockPos.containing(
                player.getX(), player.getEyeY(), player.getZ())).isEmpty();
        for (EquipmentSlot slot : EQUIPMENT) {
            luminance = Math.max(luminance, itemLuminance(player.getItemBySlot(slot), submerged));
        }
        return luminance;
    }

    public static int itemLuminance(ItemStack stack, boolean submerged) {
        if (stack.isEmpty()) {
            return 0;
        }
        if (submerged && (stack.is(Items.TORCH) || stack.is(Items.SOUL_TORCH)
                || stack.is(Items.REDSTONE_TORCH) || stack.is(Items.COPPER_TORCH))) {
            return 0;
        }
        if (stack.getItem() instanceof BlockItem blockItem) {
            return blockItem.getBlock().defaultBlockState().getLightEmission();
        }
        return stack.is(Items.LAVA_BUCKET) ? 15 : 0;
    }

    public static final class LightSampler {
        private final Level level;
        private final double daylight;
        private final Map<Long, List<LightSource>> sources;

        private LightSampler(Level level) {
            this.level = level;
            daylight = daylightFactor(level);
            sources = new HashMap<>();
            for (LightSource source : LightRules.sources(level)) {
                for (int x = section(source.x - LightMath.DYNAMIC_RADIUS); x <= section(source.x + LightMath.DYNAMIC_RADIUS); x++) {
                    for (int y = section(source.y - LightMath.DYNAMIC_RADIUS); y <= section(source.y + LightMath.DYNAMIC_RADIUS); y++) {
                        for (int z = section(source.z - LightMath.DYNAMIC_RADIUS); z <= section(source.z + LightMath.DYNAMIC_RADIUS); z++) {
                            sources.computeIfAbsent(SectionPos.asLong(x, y, z), ignored -> new ArrayList<>()).add(source);
                        }
                    }
                }
            }
        }

        public double sample(BlockPos pos) {
            double light = worldLight(level, pos, daylight);
            double x = pos.getX() + 0.5;
            double y = pos.getY() + 0.5;
            double z = pos.getZ() + 0.5;
            List<LightSource> nearby = light >= 15 ? null : sources.get(SectionPos.asLong(pos.getX() >> 4, pos.getY() >> 4, pos.getZ() >> 4));
            if (nearby != null) {
                for (LightSource source : nearby) light = Math.max(light, source.sample(x, y, z));
            }
            return light;
        }

        private static int section(double coordinate) { return (int)Math.floor(coordinate / 16.0); }
    }

    public record LightSource(long id, double x, double y, double z, int luminance) {
        public double sample(double sampleX, double sampleY, double sampleZ) {
            double dx = x - sampleX;
            double dy = y - sampleY;
            double dz = z - sampleZ;
            double distanceSquared = dx * dx + dy * dy + dz * dz;
            return distanceSquared < LightMath.DYNAMIC_RADIUS * LightMath.DYNAMIC_RADIUS
                    ? LightMath.dynamicLight(luminance, Math.sqrt(distanceSquared)) : 0;
        }
    }
}
