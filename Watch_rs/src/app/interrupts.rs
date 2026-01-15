use esp_hal::{handler, ram, timer::systimer::{SystemTimer, Unit}};

use crate::input::{handle_button_generic, handle_encoder_generic, handle_imu_int_generic};
use crate::app::runtime::DEBOUNCE_MS;

use core::sync::atomic::Ordering::Relaxed;

use super::state;

// Interrupt handler
#[handler]
#[ram]
pub fn handler() {
    let now_ms = {
        let t = SystemTimer::unit_value(
            Unit::Unit0,
        );
        t.saturating_mul(1000) / SystemTimer::ticks_per_second()
    };
    let enabled = state::inputs_enabled();

    // Button 1: handle press
    handle_button_generic(
        state::button1(),
        now_ms,
        DEBOUNCE_MS,
        || {
            if enabled {
                state::button1_flag().store(true, Relaxed);
            }
        },
    );

    // Button 2: handle press
    handle_button_generic(
        state::button2(),
        now_ms,
        DEBOUNCE_MS,
        || {
            if enabled {
                state::button2_flag().store(true, Relaxed);
            }
        },
    );

    // Button 3: handle press
    handle_button_generic(
        state::button3(),
        now_ms,
        DEBOUNCE_MS,
        || {
            if enabled {
                state::button3_flag().store(true, Relaxed);
            }
        },
    );

    // Encoder logic is fine, it's just math
    if enabled {
        handle_encoder_generic(state::rotary());
    } else {
        state::clear_encoder_interrupts();
    }

    // IMU interrupt handling
    if enabled {
        handle_imu_int_generic(state::imu_int(), state::imu_int_flag());
    } else {
        state::clear_imu_interrupt();
    }
}
