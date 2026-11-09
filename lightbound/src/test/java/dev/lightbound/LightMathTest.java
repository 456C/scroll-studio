package dev.lightbound;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.*;

class LightMathTest {
    @Test
    void moonlightCannotHoldUpThePlayerButNoonCan() {
        assertEquals(0.0, LightMath.daylightFactor(11));
        assertEquals(0.0, LightMath.daylightFactor(15));
        assertEquals(1.0, LightMath.daylightFactor(0));
        assertTrue(LightMath.daylightFactor(6) > 0);
        assertTrue(LightMath.daylightFactor(6) < 1);
    }

    @Test
    void torchHoldsTheFloorUnderYourFeetButNotDistantGround() {
        assertTrue(LightMath.isSolid(LightMath.dynamicLight(14, 2.12)));
        assertTrue(LightMath.isSolid(LightMath.dynamicLight(14, 6.0)));
        assertFalse(LightMath.isSolid(LightMath.dynamicLight(14, 7.0)));
        assertEquals(0.0, LightMath.dynamicLight(14, LightMath.DYNAMIC_RADIUS));
    }

    @Test
    void dimmerLightsHaveASmallerSolidFootprint() {
        assertTrue(LightMath.isSolid(LightMath.dynamicLight(14, 5.0)));
        assertFalse(LightMath.isSolid(LightMath.dynamicLight(7, 5.0)));
    }

    @Test
    void oneLightLevelIsTheInclusiveBoundary() {
        assertTrue(LightMath.isSolid(1.0));
        assertFalse(LightMath.isSolid(Math.nextDown(1.0)));
        assertFalse(LightMath.isSolid(0.0));
    }

    @Test
    void serverThresholdCanShrinkTheTorchSafeArea() {
        double fourBlocksAway = LightMath.dynamicLight(14, 4.0);
        assertTrue(LightMath.isSolid(fourBlocksAway, 1));
        assertFalse(LightMath.isSolid(fourBlocksAway, 8));
        assertTrue(LightMath.isSolid(8, 8));
    }

    @Test
    void heartbeatHasARepeatableShortBlackoutAndIsOffByDefault() {
        for (long tick = 0; tick < 1000; tick++) assertFalse(LightMath.heartbeatDark(tick, 0, 10));
        assertFalse(LightMath.heartbeatDark(69, 80, 10));
        assertTrue(LightMath.heartbeatDark(70, 80, 10));
        assertTrue(LightMath.heartbeatDark(79, 80, 10));
        assertFalse(LightMath.heartbeatDark(80, 80, 10));
        assertTrue(LightMath.heartbeatDark(150, 80, 10));
        assertTrue(LightMath.heartbeatDark(-1, 80, 10));
        assertTrue(LightMath.heartbeatDark(4, 5, 200));
    }
}
