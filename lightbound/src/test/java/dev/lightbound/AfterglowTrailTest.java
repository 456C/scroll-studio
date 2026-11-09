package dev.lightbound;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class AfterglowTrailTest {
    @Test
    void historyKeepsItsOriginalFootprintThenExpiresExactly() {
        AfterglowTrail trail = new AfterglowTrail();
        trail.advance(100, 40);
        trail.record(1, -16.5, 2.5, 0.5, 14, 100);
        double initial = trail.sample(-16.5, 0.5, 0.5);
        assertEquals(LightMath.dynamicLight(14, 2), initial, 1.0e-9);
        trail.advance(101, 40);
        trail.record(1, 16.5, 2.5, 0.5, 14, 101);
        assertEquals(initial, trail.sample(-16.5, 0.5, 0.5), 1.0e-9);
        assertTrue(trail.sample(16.5, 0.5, 0.5) > 0);
        trail.advance(139, 40);
        assertEquals(initial, trail.sample(-16.5, 0.5, 0.5), 1.0e-9);
        trail.advance(140, 40);
        assertEquals(0, trail.sample(-16.5, 0.5, 0.5));
        assertTrue(trail.sample(16.5, 0.5, 0.5) > 0);
        trail.advance(141, 40);
        assertTrue(trail.sources().isEmpty());
    }

    @Test
    void stationarySourcesReuseOneStableEntryAndMovementStaysBounded() {
        AfterglowTrail trail = new AfterglowTrail();
        trail.advance(0, 200);
        trail.record(1, 0, 2, 0, 14, 0);
        long id = trail.sources().getFirst().id();
        for (int tick = 1; tick < 1000; tick++) {
            trail.advance(tick, 200);
            trail.record(1, 0, 2, 0, 14, tick);
        }
        assertEquals(1, trail.sources().size());
        assertEquals(id, trail.sources().getFirst().id());
        for (int tick = 1000; tick < 3000; tick++) {
            trail.advance(tick, 200);
            trail.record(1, tick * 0.4, 2, 0, 14, tick);
            trail.record(2, -tick * 0.4, 2, 0, 15, tick);
            assertTrue(trail.sources().size() <= 2 * AfterglowTrail.MAX_SOURCES_PER_PLAYER);
        }
    }

    @Test
    void disablingAndRewindingCannotResurrectOldPaths() {
        AfterglowTrail trail = new AfterglowTrail();
        trail.advance(50, 200);
        trail.record(1, 0, 0, 0, 14, 50);
        trail.advance(51, 0);
        trail.advance(52, 200);
        assertTrue(trail.sources().isEmpty());
        trail.record(1, 0, 0, 0, 14, 52);
        trail.advance(1, 200);
        assertTrue(trail.sources().isEmpty());
        assertEquals(0, trail.sample(0, 0, 0));
    }

    @Test
    void shorteningDurationImmediatelyDropsExpiredSources() {
        AfterglowTrail trail = new AfterglowTrail();
        trail.advance(1, 200);
        trail.record(1, 0, 0, 0, 14, 1);
        trail.advance(41, 200);
        assertFalse(trail.sources().isEmpty());
        trail.advance(41, 20);
        assertTrue(trail.sources().isEmpty());
    }

    @Test
    void crowdedServersHaveOneSharedPhysicsAndNetworkHistoryBound() {
        AfterglowTrail trail = new AfterglowTrail();
        for (int tick = 0; tick < 200; tick++) {
            trail.advance(tick, 200);
            for (int player = 0; player < 110; player++) {
                trail.record(player, player * 64 + tick * 0.1, 2, 0, 14, tick);
            }
            assertTrue(trail.sources().size() <= AfterglowTrail.MAX_SOURCES_TOTAL);
        }
        assertEquals(AfterglowTrail.MAX_SOURCES_TOTAL, trail.sources().size());
        assertTrue(trail.sample(109 * 64 + 19.9, 2, 0) > 0);
        trail.clear();
        assertTrue(trail.sources().isEmpty());
    }
}
