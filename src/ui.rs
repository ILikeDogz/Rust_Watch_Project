//! UI state management and display rendering module.
//!
//! This module provides:
//! - The `UiState` enum and its navigation methods (`next`, `prev`, etc.)
//! - The `update_ui` function to render the current UI state to the display
//! - Drawing helpers for text, shapes, and layout
//!
//! Designed for use with embedded-graphics, mipidsi, and ESP-HAL display drivers.
//! All drawing is centered on a 240x240 display, but can be adapted for other sizes.


use esp_backtrace as _;

// ESP-HAL imports
use esp_hal::{
    gpio::Output,
    spi::master::Spi,
    Blocking,
};

// Display interface and device
use embedded_hal_bus::spi::ExclusiveDevice;
use mipidsi::interface::SpiInterface;                    // Provides the builder for DisplayInterface

// GC9A01 display driver
use mipidsi::{
    Display,
    models::GC9A01,
};


// Embedded-graphics
use embedded_graphics::{
    mono_font::{ascii::{FONT_10X20, FONT_6X10}, 
    MonoTextStyle, MonoTextStyleBuilder}, 
    pixelcolor::Rgb565, 
    prelude::{Point, Primitive, RgbColor, Size}, 
    primitives::{PrimitiveStyle, Rectangle, Circle, Triangle}, text::{Alignment, Baseline, Text}, 
    Drawable,
    draw_target::DrawTarget, 
};

// Display configuration, (0,0) is top-left corner
pub const RESOLUTION: u32 = 240; // 240x240 display
pub const CENTER: i32 = RESOLUTION as i32 / 2;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum UiState {
    State1,
    State2,
    State3,
    State4,
}

impl UiState {
    // All possible states
    pub const ALL: [UiState; 4] = [UiState::State1, UiState::State2, UiState::State3, UiState::State4];

    pub fn next(self) -> Self {
        use UiState::*;
        match self {
            State1 => State2,
            State2 => State3,
            State3 => State4,
            State4 => State1,
        }
    }

    pub fn prev(self) -> Self {
        use UiState::*;
        match self {
            State1 => State4,
            State2 => State1,
            State3 => State2,
            State4 => State3,
        }
    }
    
    // For potential future use: convert to/from u8 for storage
    pub fn as_u8(self) -> u8 {
        match self {
            UiState::State1 => 1,
            UiState::State2 => 2,
            UiState::State3 => 3,
            UiState::State4 => 4,
        }
    }

    // For potential future use: convert to/from u8 for storage
    pub fn from_u8(n: u8) -> Self {
        match n {
            1 => UiState::State1,
            2 => UiState::State2,
            3 => UiState::State3,
            4 => UiState::State4,
            _ => UiState::State1,
        }
    }
}

// helper function to update the display based on UI_STATE
pub fn update_ui(
    disp: &mut Display<
        SpiInterface<
            ExclusiveDevice<Spi<'_, Blocking>, Output<'_>, embedded_hal_bus::spi::NoDelay>,
            Output<'_>,
        >,
        GC9A01,
        Output<'_>,
    >,
    state: UiState,
) 
{
    // Clear display background
    disp.clear(Rgb565::BLACK).ok();

    // Get current state
    match state {
        UiState::State1 => {
            // Draw centered text
            let style_bg = MonoTextStyleBuilder::new()
                .font(&FONT_10X20)
                .text_color(Rgb565::WHITE)
                .background_color(Rgb565::GREEN)
                .build();

            Text::with_alignment(
                "State 1",
                Point::new(CENTER, CENTER),
                style_bg,
                Alignment::Center,
            )
            .draw(disp)
            .ok();
        }
        UiState::State2 => {
            // Draw a centered circle
            let diameter: u32 = 120;
            Circle::new(
                Point::new(CENTER - diameter as i32 / 2, CENTER - diameter as i32 / 2),
                diameter,
            )
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 5))
            .draw(disp)
            .ok();
        }
        UiState::State3 => {
            // Draw a filled centered rectangle
            let size = Size::new(120, 80);
            Rectangle::new(
                Point::new(CENTER - (size.width as i32 / 2), CENTER - (size.height as i32 / 2)),
                size,
            )
            .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
            .draw(disp)
            .ok();
        }
        UiState::State4 => {
            // Draw a filled centered circle
            let diameter: u32 = 120;
            Circle::new(
                Point::new(CENTER - diameter as i32 / 2, CENTER - diameter as i32 / 2),
                diameter,
            )
            .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
            .draw(disp)
            .ok();
        }

        // Rectangle::new(Point::new(0, 0), Size::new(240, 240))
        //     .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        //     .draw(&mut my_display)
        //     .ok();

        // my_display.fill_solid(
        //     &Rectangle::new(Point::new(0, 0), Size::new(240, 240)),
        //     Rgb565::BLACK
        // ).ok();

        // Rectangle::new(Point::new(0, 0), Size::new(120, 120))
        //     .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
        //     .draw(&mut my_display).ok();

        // Rectangle::new(Point::new(120, 120), Size::new(120, 120))
        //     .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
        //     .draw(&mut my_display).ok();

        // // Centered circle with diameter 160
        // let diameter: u32 = 160;
        // Circle::new(Point::new(CENTER - diameter as i32 / 2, CENTER - diameter as i32 / 2), diameter)
        //     .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 3))
        //     .draw(&mut my_display)
        //     .ok();
    }
}