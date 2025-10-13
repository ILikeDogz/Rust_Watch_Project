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
    image::{Image, ImageRaw},
};

// Display configuration, (0,0) is top-left corner
pub const RESOLUTION: u32 = 240; // 240x240 display
pub const CENTER: i32 = RESOLUTION as i32 / 2;
static MY_IMAGE: &[u8] = include_bytes!("assets/omnitrix_logo_240x240_rgb565_be.raw");

// UI State representation
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct UiState {
    pub page: Page,
    pub dialog: Option<Dialog>,
}

// Different pages in the UI
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Main(MainMenuState),
    Settings(SettingsMenuState),
    Info,
}

// Dialogs that can overlay on top of pages
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    VolumeAdjust,
    BrightnessAdjust,
    ResetSelector,
    HomePage,
    StartPage,
    AboutPage,
}

// States for Main Menu
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MainMenuState {
    Home,
    Start,
    About,
}

// States for Settings Menu
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SettingsMenuState {
    Volume,
    Brightness,
    Reset,
}

impl UiState {
    /// Switch to the next menu (Button 1)
    pub fn next_menu(self) -> Self {
        // If a dialog is open, ignore menu switching
        if self.dialog.is_some() {
            return self;
        }
        let next_page = match self.page {
            Page::Main(_) => Page::Settings(SettingsMenuState::Volume),
            Page::Settings(_) => Page::Info,
            Page::Info => Page::Main(MainMenuState::Home),
        };
        Self { page: next_page, dialog: None }
    }

    /// Move to the next item/state in the current menu (Button 3 or encoder)
    pub fn next_item(self) -> Self {
        if self.dialog.is_some() {
            return self; // Or handle dialog-specific navigation here
        }
        let next_page = match self.page {
            Page::Main(state) => {
                let next = match state {
                    MainMenuState::Home => MainMenuState::Start,
                    MainMenuState::Start => MainMenuState::About,
                    MainMenuState::About => MainMenuState::Home,
                };
                Page::Main(next)
            }
            Page::Settings(state) => {
                let next = match state {
                    SettingsMenuState::Volume => SettingsMenuState::Brightness,
                    SettingsMenuState::Brightness => SettingsMenuState::Reset,
                    SettingsMenuState::Reset => SettingsMenuState::Volume,
                };
                Page::Settings(next)
            }
            Page::Info => Page::Info,
        };
        Self { page: next_page, dialog: None }
    }

    /// Move to the previous item/state in the current menu
    pub fn prev_item(self) -> Self {
        if self.dialog.is_some() {
            return self; // Or handle dialog-specific navigation here
        }
        let prev_page = match self.page {
            Page::Main(state) => {
                let prev = match state {
                    MainMenuState::Home => MainMenuState::About,
                    MainMenuState::Start => MainMenuState::Home,
                    MainMenuState::About => MainMenuState::Start,
                };
                Page::Main(prev)
            }
            Page::Settings(state) => {
                let prev = match state {
                    SettingsMenuState::Volume => SettingsMenuState::Reset,
                    SettingsMenuState::Brightness => SettingsMenuState::Volume,
                    SettingsMenuState::Reset => SettingsMenuState::Brightness,
                };
                Page::Settings(prev)
            }
            Page::Info => Page::Info,
        };
        Self { page: prev_page, dialog: None }
    }

    /// Select current item (Button 2)
    pub fn select(self) -> Self {
        // If a dialog is open, close it and return to the underlying page
        if let Some(_) = self.dialog {
            return Self { page: self.page, dialog: None };
        }
        // Otherwise, open a dialog based on the current page/item
        let dialog = match self.page {
            Page::Main(MainMenuState::Home) => Some(Dialog::HomePage),
            Page::Main(MainMenuState::Start) => Some(Dialog::StartPage),
            Page::Main(MainMenuState::About) => Some(Dialog::AboutPage),
            Page::Settings(SettingsMenuState::Volume) => Some(Dialog::VolumeAdjust),
            Page::Settings(SettingsMenuState::Brightness) => Some(Dialog::BrightnessAdjust),
            Page::Settings(SettingsMenuState::Reset) => Some(Dialog::ResetSelector),
            Page::Info => None, // Or maybe Some(Dialog::AboutPage)
        };
        Self { page: self.page, dialog }
    }
}

// helper function to draw centered text
fn draw_text(
    disp: &mut Display<
        SpiInterface<
            ExclusiveDevice<Spi<'_, Blocking>, Output<'_>, embedded_hal_bus::spi::NoDelay>,
            Output<'_>,
        >,
        GC9A01,
        Output<'_>,
    >,
    text: &str,
    fg: Rgb565,
    bg: Rgb565,
    x_point: i32,
    y_point: i32,
) {
    let style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(fg)
        .background_color(bg)
        .build();

    Text::with_alignment(
        text,
        Point::new(x_point, y_point),
        style,
        Alignment::Center,
    )
    .draw(disp)
    .ok();
}

// helper function to draw a centered image
fn draw_image(
    disp: &mut Display<
        SpiInterface<
            ExclusiveDevice<Spi<'_, Blocking>, Output<'_>, embedded_hal_bus::spi::NoDelay>,
            Output<'_>,
        >,
        GC9A01,
        Output<'_>,
    >,
    image_data: &'static [u8],
    width: u32,
    height: u32,
) {
    // Create an ImageRaw object (assuming RGB565 format)
    let raw = ImageRaw::<Rgb565>::new(image_data, width);

    // Center the image
    let x = (RESOLUTION - width) as i32 / 2;
    let y = (RESOLUTION - height) as i32 / 2;

    // Draw the image
    Image::new(&raw, Point::new(x, y))
        .draw(disp)
        .ok();
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
    disp.clear(Rgb565::BLACK).ok();

    if let Some(dialog) = state.dialog {
        match dialog {
            Dialog::VolumeAdjust =>
                draw_text(disp, "Adjust Volume (TEMP)", Rgb565::WHITE, Rgb565::RED, CENTER, CENTER),
            Dialog::BrightnessAdjust =>
                draw_text(disp, "Adjust Brightness (TEMP)", Rgb565::WHITE, Rgb565::MAGENTA, CENTER, CENTER),
            Dialog::ResetSelector =>
                draw_text(disp, "Reset? (TEMP)", Rgb565::WHITE, Rgb565::YELLOW, CENTER, CENTER),
            Dialog::HomePage =>
                draw_text(disp, "Home Page (TEMP)", Rgb565::GREEN, Rgb565::BLACK, CENTER, CENTER),
            Dialog::StartPage =>
                draw_text(disp, "Start Page (TEMP)", Rgb565::BLUE, Rgb565::BLACK, CENTER, CENTER),
            Dialog::AboutPage =>
                draw_text(disp, "About Page (TEMP)", Rgb565::CYAN, Rgb565::BLACK, CENTER, CENTER),
        }
        return;
    }

    match state.page {
        Page::Main(menu_state) => {
            let (msg, fg, bg) = match menu_state {
                MainMenuState::Home  => ("Main: Home", Rgb565::WHITE, Rgb565::GREEN),
                MainMenuState::Start => ("Main: Start", Rgb565::WHITE, Rgb565::GREEN),
                MainMenuState::About => ("Main: About", Rgb565::WHITE, Rgb565::GREEN),
            };
            draw_text(disp, msg, fg, bg, CENTER, CENTER);
        }
        Page::Settings(settings_state) => {
            let (msg, fg, bg) = match settings_state {
                SettingsMenuState::Volume     => ("Settings: Volume", Rgb565::YELLOW, Rgb565::BLUE),
                SettingsMenuState::Brightness => ("Settings: Brightness", Rgb565::YELLOW, Rgb565::BLUE),
                SettingsMenuState::Reset      => ("Settings: Reset", Rgb565::YELLOW, Rgb565::BLUE),
            };
            draw_text(disp, msg, fg, bg, CENTER, CENTER);
        }
        Page::Info => {
            // draw_text(disp, "Info Screen", Rgb565::CYAN, Rgb565::BLACK, CENTER, CENTER);
            draw_image(disp, MY_IMAGE, 240, 240);
        }
    }
}