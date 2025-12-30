// Timer-related utilities for the HAL

use esp_hal::timer::systimer::{SystemTimer, Unit};

// A delay provider that uses the ESP32-S3's SystemTimer.
pub struct TimerDelay;

impl embedded_hal::delay::DelayNs for TimerDelay {
    #[inline]
    fn delay_ns(&mut self, ns: u32) {
        let ticks_per_sec = SystemTimer::ticks_per_second();
        let start = SystemTimer::unit_value(Unit::Unit0);
        let delta_ticks = (ns as u64 * ticks_per_sec).div_ceil(1_000_000_000);
        let end_ticks = start.saturating_add(delta_ticks);

        while SystemTimer::unit_value(Unit::Unit0) < end_ticks {
            core::hint::spin_loop();
        }
    }

    #[inline]
    fn delay_us(&mut self, us: u32) {
        let ticks_per_sec = SystemTimer::ticks_per_second();
        let start = SystemTimer::unit_value(Unit::Unit0);
        let delta_ticks = (us as u64 * ticks_per_sec).div_ceil(1_000_000);
        let end_ticks = start.saturating_add(delta_ticks);

        while SystemTimer::unit_value(Unit::Unit0) < end_ticks {
            core::hint::spin_loop();
        }
    }

    #[inline]
    fn delay_ms(&mut self, ms: u32) {
        let ticks_per_sec = SystemTimer::ticks_per_second();
        let start = SystemTimer::unit_value(Unit::Unit0);
        let delta_ticks = (ms as u64 * ticks_per_sec).div_ceil(1_000);
        let end_ticks = start.saturating_add(delta_ticks);

        while SystemTimer::unit_value(Unit::Unit0) < end_ticks {
            core::hint::spin_loop();
        }
    }
}
