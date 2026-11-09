package dev.lightbound;

import dev.lightbound.client.AuraParticles;
import java.util.List;

/** Emitter invariants, run on the client thread alongside the rendered-world checks. */
public final class AuraParticleChecks {
    private AuraParticleChecks() {}

    public static void run() {
        var first = List.of(new VoidEdgeScanner.Edge(0, 80, 0, 1, 80, 0));
        var moved = List.of(new VoidEdgeScanner.Edge(10, 80, 0, 11, 80, 0));
        try {
            AuraParticles.reset();
            var initial = AuraParticles.update(null, first, 1, true, 1);
            check(!initial.isEmpty(), "An enabled confirmed boundary must emit sparks");
            check(initial.size() <= AuraParticles.MAX_SPAWNS_PER_TICK, "Spawn rate exceeded");
            for (int frame = 0; frame < 100; frame++) {
                check(AuraParticles.update(null, first, 1, true, 1) == initial,
                        "Extra rendered frames must reuse the particle snapshot");
            }
            for (int tick = 2; tick < 1000; tick++) {
                var sparks = AuraParticles.update(null, first, tick, true, 1);
                check(sparks.size() <= AuraParticles.MAX_PARTICLES, "Live particle limit exceeded");
                for (var spark : sparks) {
                    check(spark.x() >= 0 && spark.x() <= 1 && Math.abs(spark.z()) < 0.1,
                            "A spark escaped its confirmed source edge");
                    check(spark.y() > 80 && spark.y() < 80.45,
                            "A spark outlived its bounded rising trail");
                    check(spark.opacity() > 0 && spark.opacity() <= 1, "Invalid particle fade");
                }
            }
            check(AuraParticles.update(null, moved, 999, true, 1).isEmpty(),
                    "Moving the boundary must remove stale particles in the same frame");
            check(!AuraParticles.update(null, moved, 1000, true, 1).isEmpty(),
                    "The new boundary should resume emission on the next tick");
            check(AuraParticles.update(null, moved, 1000, false, 1).isEmpty()
                            && AuraParticles.liveCount() == 0,
                    "Turning particles off must immediately clear existing particles");
            AuraParticles.update(null, first, 1001, true, 1);
            check(AuraParticles.update(null, first, 1001, true, 0).isEmpty(),
                    "Zero density must stop all particle work");
            AuraParticles.update(null, first, 1002, true, 1);
            check(AuraParticles.update(null, List.of(), 1002, true, 1).isEmpty(),
                    "No confirmed edge means no aura particles");
        } finally {
            AuraParticles.reset();
        }
    }

    private static void check(boolean condition, String message) {
        if (!condition) throw new AssertionError(message);
    }
}
