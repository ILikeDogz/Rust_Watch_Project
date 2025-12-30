use esp_hal::handler;
use esp_hal::ram;

use crate::input::{handle_button_generic, handle_encoder_generic, handle_imu_int_generic};

use super::state;

// Interrupt handler
#[handler]
#[ram]
pub fn handler() {
    let now_ms = {
        let t = esp_hal::timer::systimer::SystemTimer::unit_value(
            esp_hal::timer::systimer::Unit::Unit0,
        );
        t.saturating_mul(1000) / esp_hal::timer::systimer::SystemTimer::ticks_per_second()
    };

    // Button 1: handle press
    handle_button_generic(
        state::button1(),
        now_ms,
        crate::app::runtime::DEBOUNCE_MS,
        || {
            state::button1_flag().store(true, core::sync::atomic::Ordering::Relaxed);
        },
    );

    // Button 2: handle press
    handle_button_generic(
        state::button2(),
        now_ms,
        crate::app::runtime::DEBOUNCE_MS,
        || {
            state::button2_flag().store(true, core::sync::atomic::Ordering::Relaxed);
        },
    );

    // Button 3: handle press
    handle_button_generic(
        state::button3(),
        now_ms,
        crate::app::runtime::DEBOUNCE_MS,
        || {
            state::button3_flag().store(true, core::sync::atomic::Ordering::Relaxed);
        },
    );

    // Encoder logic is fine, it's just math
    handle_encoder_generic(state::rotary());

    // IMU interrupt handling
    handle_imu_int_generic(state::imu_int(), state::imu_int_flag());
}
