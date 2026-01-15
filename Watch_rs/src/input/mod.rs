// Input handling module for buttons and rotary encoder and IMU interrupts.
//
// This module provides:
// - `ButtonState` and debounced button handling via `handle_button_generic`
// - `RotaryState` and quadrature decoding via `handle_encoder_generic`
// - `ImuIntState` and IMU interrupt handling via `handle_imu_int_generic`
//
// All input state is protected with `critical_section` for safe concurrent access in interrupt and main contexts.
// Designed for use with ESP-HAL GPIO and embedded Rust applications.

use esp_backtrace as _;

pub mod buttons;
pub mod encoder;
pub mod imu_int;

pub use buttons::{handle_button_generic, ButtonState};
pub use encoder::{handle_encoder_generic, RotaryState};
pub use imu_int::{handle_imu_int_generic, ImuIntState};
