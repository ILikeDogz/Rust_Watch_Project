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
pub struct UiState {
    pub page: Page,
    pub dialog: Option<Dialog>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Main(MainMenuState),
    Settings(SettingsMenuState),
    Info,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    VolumeAdjust,
    BrightnessAdjust,
    ResetSelector,
    HomePage,
    StartPage,
    AboutPage,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MainMenuState {
    Home,
    Start,
    About,
}

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

    // If a dialog is open, render it and return
    if let Some(dialog) = state.dialog {
        match dialog {
            Dialog::VolumeAdjust => {
                let style = MonoTextStyleBuilder::new()
                    .font(&FONT_10X20)
                    .text_color(Rgb565::WHITE)
                    .background_color(Rgb565::RED)
                    .build();
                Text::with_alignment(
                    "Adjust Volume (TEMP)",
                    Point::new(CENTER, CENTER),
                    style,
                    Alignment::Center,
                )
                .draw(disp)
                .ok();
            }
            Dialog::BrightnessAdjust => {
                let style = MonoTextStyleBuilder::new()
                    .font(&FONT_10X20)
                    .text_color(Rgb565::WHITE)
                    .background_color(Rgb565::MAGENTA)
                    .build();
                Text::with_alignment(
                    "Adjust Brightness (TEMP)",
                    Point::new(CENTER, CENTER),
                    style,
                    Alignment::Center,
                )
                .draw(disp)
                .ok();
            }
            Dialog::ResetSelector => {
                let style = MonoTextStyleBuilder::new()
                    .font(&FONT_10X20)
                    .text_color(Rgb565::WHITE)
                    .background_color(Rgb565::YELLOW)
                    .build();
                Text::with_alignment(
                    "Reset? (TEMP)",
                    Point::new(CENTER, CENTER),
                    style,
                    Alignment::Center,
                )
                .draw(disp)
                .ok();
            }
            Dialog::HomePage => {
                let style = MonoTextStyleBuilder::new()
                    .font(&FONT_10X20)
                    .text_color(Rgb565::GREEN)
                    .background_color(Rgb565::BLACK)
                    .build();
                Text::with_alignment(
                    "Home Page (TEMP)",
                    Point::new(CENTER, CENTER),
                    style,
                    Alignment::Center,
                )
                .draw(disp)
                .ok();
            }
            Dialog::StartPage => {
                let style = MonoTextStyleBuilder::new()
                    .font(&FONT_10X20)
                    .text_color(Rgb565::BLUE)
                    .background_color(Rgb565::BLACK)
                    .build();
                Text::with_alignment(
                    "Start Page (TEMP)",
                    Point::new(CENTER, CENTER),
                    style,
                    Alignment::Center,
                )
                .draw(disp)
                .ok();
            }
            Dialog::AboutPage => {
                let style = MonoTextStyleBuilder::new()
                    .font(&FONT_10X20)
                    .text_color(Rgb565::CYAN)
                    .background_color(Rgb565::BLACK)
                    .build();
                Text::with_alignment(
                    "About Page (TEMP)",
                    Point::new(CENTER, CENTER),
                    style,
                    Alignment::Center,
                )
                .draw(disp)
                .ok();
            }
        }
        return;
    }

    // Otherwise, render the current page
    match state.page {
        Page::Main(menu_state) => {
            let msg = match menu_state {
                MainMenuState::Home => "Main: Home",
                MainMenuState::Start => "Main: Start",
                MainMenuState::About => "Main: About",
            };

            let style = MonoTextStyleBuilder::new()
                .font(&FONT_10X20)
                .text_color(Rgb565::WHITE)
                .background_color(Rgb565::GREEN)
                .build();

            Text::with_alignment(
                msg,
                Point::new(CENTER, CENTER),
                style,
                Alignment::Center,
            )
            .draw(disp)
            .ok();
        }
        Page::Settings(settings_state) => {
            let msg = match settings_state {
                SettingsMenuState::Volume => "Settings: Volume",
                SettingsMenuState::Brightness => "Settings: Brightness",
                SettingsMenuState::Reset => "Settings: Reset",
            };

            let style = MonoTextStyleBuilder::new()
                .font(&FONT_10X20)
                .text_color(Rgb565::YELLOW)
                .background_color(Rgb565::BLUE)
                .build();

            Text::with_alignment(
                msg,
                Point::new(CENTER, CENTER),
                style,
                Alignment::Center,
            )
            .draw(disp)
            .ok();
        }
        Page::Info => {
            let style = MonoTextStyleBuilder::new()
                .font(&FONT_10X20)
                .text_color(Rgb565::CYAN)
                .background_color(Rgb565::BLACK)
                .build();

            Text::with_alignment(
                "Info Screen",
                Point::new(CENTER, CENTER),
                style,
                Alignment::Center,
            )
            .draw(disp)
            .ok();
        }
    }
}