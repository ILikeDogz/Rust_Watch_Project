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
// use esp_hal::{
//     gpio::Output,
//     spi::master::Spi,
//     Blocking,
// };


// Embedded-graphics
use embedded_graphics::{
    Drawable, draw_target::DrawTarget, image::{Image, ImageRaw, ImageRawBE}, mono_font::{MonoTextStyle, MonoTextStyleBuilder, ascii::{FONT_6X10, FONT_10X20}}, pixelcolor::Rgb565, prelude::{OriginDimensions, Point, Primitive, RgbColor, Size}, primitives::{Circle, PrimitiveStyle, Rectangle, Triangle}, text::{Alignment, Baseline, Text}
};
use miniz_oxide::inflate::decompress_to_vec_zlib;


// Make a lightweight trait bound we’ll use for the factory’s return type.

pub trait PanelRgb565: DrawTarget<Color = Rgb565> + OriginDimensions {}
impl<T> PanelRgb565 for T where T: DrawTarget<Color = Rgb565> + OriginDimensions {}

// static TRANSFORM_FLASH: AtomicU8 = AtomicU8::new(0);

// #[cfg(feature = "devkit-esp32s3-disp128")]
// type DisplayType<'a> = Display<
//     SpiInterface<'a,
//         ExclusiveDevice<Spi<'a, Blocking>, Output<'a>, embedded_hal_bus::spi::NoDelay>,
//         Output<'a>,
//     >,
//     GC9A01,
//     Output<'a>,
// >;

// #[cfg(feature = "esp32s3-disp143Oled")]
// type DisplayType<'a> = crate::co5300::DisplayType<'a>;

// Display configuration, (0,0) is top-left corner
#[cfg(feature = "devkit-esp32s3-disp128")]
pub const RESOLUTION: u32 = 240;

#[cfg(feature = "esp32s3-disp143Oled")]
pub const RESOLUTION: u32 = 466;

pub const CENTER: i32 = (RESOLUTION / 2) as i32;

// Feature-selected image dimensions (adjust OLED to 466 if you have 466×466 assets)
#[cfg(feature = "devkit-esp32s3-disp128")]
pub const IMG_W: u32 = 240;
#[cfg(feature = "devkit-esp32s3-disp128")]
pub const IMG_H: u32 = 240;

#[cfg(feature = "esp32s3-disp143Oled")]
pub const IMG_W: u32 = 466; // change to 466 if you add 466×466 assets
#[cfg(feature = "esp32s3-disp143Oled")]
pub const IMG_H: u32 = 466; // change to 466 if you add 466×466 assets

// Compile-time suffix for asset filenames
#[cfg(feature = "devkit-esp32s3-disp128")]
macro_rules! res { () => { "240x240" } }

#[cfg(feature = "esp32s3-disp143Oled")]
macro_rules! res { () => { "466x466" } } // set to "466x466" when you have OLED-sized assets

// Feature-picked assets
// static MY_IMAGE: &[u8]    = include_bytes!(concat!("assets/omnitrix_logo_", res!(), "_rgb565_be.raw"));
static ALIEN1_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien1_",  res!(), "_rgb565_be.raw.zlib"));
static ALIEN2_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien2_",  res!(), "_rgb565_be.raw.zlib"));
static ALIEN3_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien3_",  res!(), "_rgb565_be.raw.zlib"));
static ALIEN4_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien4_",  res!(), "_rgb565_be.raw.zlib"));
static ALIEN5_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien5_",  res!(), "_rgb565_be.raw.zlib"));
static ALIEN6_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien6_",  res!(), "_rgb565_be.raw.zlib"));
static ALIEN7_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien7_",  res!(), "_rgb565_be.raw.zlib"));
static ALIEN8_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien8_",  res!(), "_rgb565_be.raw.zlib"));
static ALIEN9_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien9_",  res!(), "_rgb565_be.raw.zlib"));
static ALIEN10_IMAGE: &[u8] = include_bytes!(concat!("assets/alien10_", res!(), "_rgb565_be.raw.zlib"));

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
    Omnitrix(OmnitrixState),
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
    TransformPage,
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

// States for Omnitrix Menu
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OmnitrixState {
    Alien1,
    Alien2,
    Alien3,
    Alien4,
    Alien5,
    Alien6,
    Alien7,
    Alien8,
    Alien9,
    Alien10,
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
            Page::Settings(_) => Page::Omnitrix(OmnitrixState::Alien1),
            Page::Omnitrix(_) => Page::Info,
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
            Page::Omnitrix(state) => {
                let next = match state {
                    OmnitrixState::Alien1 => OmnitrixState::Alien2,
                    OmnitrixState::Alien2 => OmnitrixState::Alien3,
                    OmnitrixState::Alien3 => OmnitrixState::Alien4,
                    OmnitrixState::Alien4 => OmnitrixState::Alien5,
                    OmnitrixState::Alien5 => OmnitrixState::Alien6,
                    OmnitrixState::Alien6 => OmnitrixState::Alien7,
                    OmnitrixState::Alien7 => OmnitrixState::Alien8,
                    OmnitrixState::Alien8 => OmnitrixState::Alien9,
                    OmnitrixState::Alien9 => OmnitrixState::Alien10,
                    OmnitrixState::Alien10 => OmnitrixState::Alien1,
                };
                Page::Omnitrix(next)
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
            Page::Omnitrix(state) => {
                let prev = match state {
                    OmnitrixState::Alien1 => OmnitrixState::Alien10,
                    OmnitrixState::Alien2 => OmnitrixState::Alien1,
                    OmnitrixState::Alien3 => OmnitrixState::Alien2,
                    OmnitrixState::Alien4 => OmnitrixState::Alien3,
                    OmnitrixState::Alien5 => OmnitrixState::Alien4,
                    OmnitrixState::Alien6 => OmnitrixState::Alien5,
                    OmnitrixState::Alien7 => OmnitrixState::Alien6,
                    OmnitrixState::Alien8 => OmnitrixState::Alien7,
                    OmnitrixState::Alien9 => OmnitrixState::Alien8,
                    OmnitrixState::Alien10 => OmnitrixState::Alien9,
                };
                Page::Omnitrix(prev)
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
            Page::Omnitrix(_) => Some(Dialog::TransformPage),
            Page::Info => None, // Or maybe Some(Dialog::AboutPage)
        };
        Self { page: self.page, dialog }
    }
}

// helper function to draw centered text
fn draw_text(
    disp: &mut impl PanelRgb565,
    text: &str,
    fg: Rgb565,
    bg: Rgb565,
    x_point: i32,
    y_point: i32,
    clear: bool,
) {
    if clear {
        // Clear the display with background color
        disp.clear(Rgb565::BLACK).ok();
    }
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
fn draw_image_runtime(
    disp: &mut impl PanelRgb565,
    data_zlib: &'static [u8],
    w: u32,
    h: u32,
    clear: bool,
) {
    if clear { disp.clear(Rgb565::BLACK).ok(); }

    // Decompress into PSRAM
    let bytes = decompress_to_vec_zlib(data_zlib).unwrap_or_default();
    if bytes.len() != (w * h * 2) as usize { return; }

    // Generic path (works on GC9A01 too)
    let raw = ImageRawBE::<Rgb565>::new(&bytes, w);
    let x = (RESOLUTION - w) as i32 / 2;
    let y = (RESOLUTION - h) as i32 / 2;
    Image::new(&raw, Point::new(x, y)).draw(disp).ok();

    // If on CO5300, you can call the fast blit instead (more efficient):
    // if let Some(co) = (<&mut dyn core::any::Any>::from(disp)).downcast_mut::<crate::co5300::DisplayType<'_>>() {
    //     let _ = co.blit_image_be_centered(w as u16, h as u16, &bytes);
    // }
}


// helper function to update the display based on UI_STATE
pub fn update_ui(
    disp: &mut impl PanelRgb565,
    state: UiState,
)
{
    if let Some(dialog) = state.dialog {
        match dialog {
            Dialog::VolumeAdjust =>
                draw_text(disp, "Adjust Volume (TEMP)", Rgb565::WHITE, Rgb565::RED, CENTER, CENTER, true),
            Dialog::BrightnessAdjust =>
                draw_text(disp, "Adjust Brightness (TEMP)", Rgb565::WHITE, Rgb565::MAGENTA, CENTER, CENTER, true),
            Dialog::ResetSelector =>
                draw_text(disp, "Reset? (TEMP)", Rgb565::WHITE, Rgb565::YELLOW, CENTER, CENTER, true),
            Dialog::HomePage =>
                draw_text(disp, "Home Page (TEMP)", Rgb565::GREEN, Rgb565::BLACK, CENTER, CENTER, true),
            Dialog::StartPage =>
                draw_text(disp, "Start Page (TEMP)", Rgb565::BLUE, Rgb565::BLACK, CENTER, CENTER, true),
            Dialog::AboutPage =>
                draw_text(disp, "About Page (TEMP)", Rgb565::CYAN, Rgb565::BLACK, CENTER, CENTER, true),
            Dialog::TransformPage => {
                let style = PrimitiveStyle::with_fill(Rgb565::GREEN);
                let diameter: u32 = 240;
                Circle::new(Point::new(CENTER - diameter as i32 / 2, CENTER - diameter as i32 / 2), diameter)
                    .into_styled(style)
                    .draw(disp)
                    .ok();
            }
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
            draw_text(disp, msg, fg, bg, CENTER, CENTER, true);
        }
        Page::Settings(settings_state) => {
            let (msg, fg, bg) = match settings_state {
                SettingsMenuState::Volume     => ("Settings: Volume", Rgb565::YELLOW, Rgb565::BLUE),
                SettingsMenuState::Brightness => ("Settings: Brightness", Rgb565::YELLOW, Rgb565::BLUE),
                SettingsMenuState::Reset      => ("Settings: Reset", Rgb565::YELLOW, Rgb565::BLUE),
            };
            draw_text(disp, msg, fg, bg, CENTER, CENTER, true);
        }
        // Page::Omnitrix(omnitrix_state) => {
        //     let (msg, fg, bg) = match omnitrix_state {
        //         OmnitrixState::Alien1  => ("Omnitrix: Alien 1", Rgb565::BLACK, Rgb565::WHITE),
        //         OmnitrixState::Alien2  => ("Omnitrix: Alien 2", Rgb565::BLACK, Rgb565::WHITE),
        //         OmnitrixState::Alien3  => ("Omnitrix: Alien 3", Rgb565::BLACK, Rgb565::WHITE),
        //         OmnitrixState::Alien4  => ("Omnitrix: Alien 4", Rgb565::BLACK, Rgb565::WHITE),
        //         OmnitrixState::Alien5  => ("Omnitrix: Alien 5", Rgb565::BLACK, Rgb565::WHITE),
        //         OmnitrixState::Alien6  => ("Omnitrix: Alien 6", Rgb565::BLACK, Rgb565::WHITE),
        //         OmnitrixState::Alien7  => ("Omnitrix: Alien 7", Rgb565::BLACK, Rgb565::WHITE),
        //         OmnitrixState::Alien8  => ("Omnitrix: Alien 8", Rgb565::BLACK, Rgb565::WHITE),
        //         OmnitrixState::Alien9  => ("Omnitrix: Alien 9", Rgb565::BLACK, Rgb565::WHITE),
        //         OmnitrixState::Alien10 => ("Omnitrix: Alien 10", Rgb565::BLACK, Rgb565::WHITE),
        //     };
        //     draw_text(disp, msg, fg, bg, CENTER, CENTER, true);
        // }
        Page::Omnitrix(omnitrix_state) => {
            let (_msg, image) = match omnitrix_state {
                OmnitrixState::Alien1  => ("Omnitrix: Alien 1", ALIEN1_IMAGE),
                OmnitrixState::Alien2  => ("Omnitrix: Alien 2", ALIEN2_IMAGE),
                OmnitrixState::Alien3  => ("Omnitrix: Alien 3", ALIEN3_IMAGE),
                OmnitrixState::Alien4  => ("Omnitrix: Alien 4", ALIEN4_IMAGE),
                OmnitrixState::Alien5  => ("Omnitrix: Alien 5", ALIEN5_IMAGE),
                OmnitrixState::Alien6  => ("Omnitrix: Alien 6", ALIEN6_IMAGE),
                OmnitrixState::Alien7  => ("Omnitrix: Alien 7", ALIEN7_IMAGE),
                OmnitrixState::Alien8  => ("Omnitrix: Alien 8", ALIEN8_IMAGE),
                OmnitrixState::Alien9  => ("Omnitrix: Alien 9", ALIEN9_IMAGE),
                OmnitrixState::Alien10 => ("Omnitrix: Alien 10", ALIEN10_IMAGE),
            };
            draw_image_runtime(disp, image, IMG_W, IMG_H, false);
            // Optionally, overlay the name as text:
            // draw_text(disp, msg, Rgb565::BLACK, Rgb565::WHITE, CENTER, 20);
        }
        
        Page::Info => {
            draw_text(disp, "Info Screen", Rgb565::CYAN, Rgb565::BLACK, CENTER, CENTER, true);
            // draw_image(disp, MY_IMAGE, IMG_W, IMG_H, false);
        }
    }
}