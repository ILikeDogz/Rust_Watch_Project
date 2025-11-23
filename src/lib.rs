#![no_std]

pub mod display;
pub mod input;
pub mod ui;
pub mod wiring;

#[cfg(feature = "esp32s3-disp143Oled")]
pub mod co5300;
