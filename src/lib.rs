#![no_std]

pub mod wiring;
pub mod input;
pub mod ui;
pub mod display;

#[cfg(feature = "esp32s3-disp143Oled")]
pub mod co5300;