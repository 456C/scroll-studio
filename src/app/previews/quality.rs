//! Hysteresis avoids oscillation; delayed repaint only runs while recovering.
#[derive(Default)]
pub(super) struct PreviewQuality {
    level: u8,
    slow: u8,
    settle_until: f64,
    last_interaction: f64,
    idle_attempts: u8,
    last_idle_check: f64,
}
impl PreviewQuality {
    pub fn observe(&mut self, seconds: f32, interacting: bool, now: f64) -> bool {
        if interacting {
            self.last_interaction = now;
            self.idle_attempts = 0;
            self.last_idle_check = now;
        } else if self.level > 0 && now - self.last_idle_check >= 0.7 {
            self.idle_attempts = self.idle_attempts.saturating_add(1);
            self.last_idle_check = now;
        }
        if seconds.is_finite() && seconds > 0.024 {
            self.slow = self
                .slow
                .saturating_add(if seconds > 0.050 { 2 } else { 1 });
            self.settle_until = now + 1.5;
            if self.slow >= 3 {
                self.level = (self.level + 1).min(2);
                self.slow = 0;
            }
        } else {
            self.slow = self.slow.saturating_sub(1);
        }
        if self.level > 0 && now >= self.settle_until && now - self.last_interaction >= 0.6 {
            self.level -= 1;
            self.settle_until = now + 0.8;
        }
        self.level > 0 && self.idle_attempts < 4
    }
    pub fn scale(&self) -> f32 {
        match self.level {
            0 => 1.0,
            1 => 0.65,
            _ => 0.4,
        }
    }
    pub fn limit(&self, side: u32) -> u32 {
        side.min(match self.level {
            0 => 2048,
            1 => 1536,
            _ => 768,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sustained_slow_frames_reduce_preview_quality_and_idle_restores_it() {
        let mut q = PreviewQuality::default();
        assert!(!q.observe(0.004, true, 0.0));
        assert_eq!(q.limit(2048), 2048);
        for n in 1..=6 {
            q.observe(0.060, true, n as f64 * 0.06);
        }
        assert_eq!(q.limit(2048), 768);
        assert_eq!(q.limit(384), 384);
        q.observe(0.004, true, 2.0);
        assert_eq!(q.limit(2048), 768);
        assert!(q.observe(0.004, false, 2.7));
        assert_eq!(q.limit(2048), 1536);
        assert!(!q.observe(0.004, false, 3.6));
        assert_eq!(q.limit(2048), 2048);
    }
    #[test]
    fn isolated_spike_does_not_oscillate() {
        let mut q = PreviewQuality::default();
        q.observe(0.030, false, 0.0);
        q.observe(0.005, false, 0.1);
        assert_eq!(q.limit(2048), 2048);
    }
    #[test]
    fn texture_completion_bursts_do_not_use_up_idle_recovery_attempts() {
        let mut q = PreviewQuality::default();
        for n in 0..6 {
            q.observe(0.060, true, n as f64 * 0.02);
        }
        assert_eq!(q.limit(2048), 768);
        for n in 0..30 {
            assert!(q.observe(0.004, false, 0.2 + n as f64 * 0.01));
        }
        q.observe(0.004, false, 1.7);
        q.observe(0.004, false, 2.6);
        assert_eq!(q.limit(2048), 2048);
    }
}
