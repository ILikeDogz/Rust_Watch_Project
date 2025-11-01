//! UI state management and display rendering module.
//!
//! This module provides:
//! - The `UiState` enum and its navigation methods (`next`, `prev`, etc.)
//! - The `update_ui` function to render the current UI state to the display
//! - Drawing helpers for text, shapes, and layout
//!
//! Designed for use with embedded-graphics, mipidsi, and ESP-HAL display drivers.
//! All drawing is centered on a 240x240 display, but can be adapted for other sizes.

#[cfg(feature = "esp32s3-disp143Oled")]
extern crate alloc;
#[cfg(feature = "esp32s3-disp143Oled")]
use alloc::vec::Vec;
#[cfg(feature = "esp32s3-disp143Oled")]
use core::{any::Any, cell::RefCell};
#[cfg(feature = "esp32s3-disp143Oled")]
use critical_section::Mutex;
#[cfg(feature = "esp32s3-disp143Oled")]
const CACHE_CAP: usize = 3;


use esp_backtrace as _;

// ESP-HAL imports
// use esp_hal::{
//     gpio::Output,
//     spi::master::Spi,
//     Blocking,
// };


// Embedded-graphics
use embedded_graphics::{
    Drawable, draw_target::DrawTarget, image::{Image, ImageRaw, ImageRawBE}, mono_font::{MonoTextStyle, MonoTextStyleBuilder, ascii::{FONT_6X10, FONT_10X20}}, pixelcolor::{Rgb565, raw::RawU16}, prelude::{OriginDimensions, Point, Primitive, RgbColor, Size}, primitives::{Circle, PrimitiveStyle, Rectangle, Triangle}, text::{Alignment, Baseline, Text}
};
use miniz_oxide::inflate::decompress_to_vec_zlib;


// Make a lightweight trait bound we’ll use for the factory’s return type.
pub trait PanelRgb565: DrawTarget<Color = Rgb565> + OriginDimensions + Any {}
impl<T> PanelRgb565 for T where T: DrawTarget<Color = Rgb565> + OriginDimensions + Any {}


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

// // apart of debug to try and speed up display
// fn draw_image_runtime(
//     disp: &mut impl PanelRgb565,
//     data_zlib: &'static [u8],
//     w: u32,
//     h: u32,
//     clear: bool,
// ) {
//     if clear { let _ = disp.clear(Rgb565::BLACK); }

//     let bytes = decompress_to_vec_zlib(data_zlib).unwrap_or_default();
//     if bytes.len() != (w * h * 2) as usize { return; }

//     let x = (RESOLUTION.saturating_sub(w)) as i32 / 2;
//     let y = (RESOLUTION.saturating_sub(h)) as i32 / 2;

//     // OLED fast path: one window + one RAMWR, zero extra copies
//     #[cfg(feature = "esp32s3-disp143Oled")]
//     if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::co5300::DisplayType<'static>>() {
//         let _ = co.blit_rect_be_fast(x as u16, y as u16, w as u16, h as u16, &bytes);
//         return;
//     }

//     // Fallbacks
//     #[cfg(feature = "devkit-esp32s3-disp128")]
//     {
//         let raw = ImageRawBE::<Rgb565>::new(&bytes, w);
//         let _ = Image::new(&raw, Point::new(x, y)).draw(disp);
//         return;
//     }
//     let area = Rectangle::new(Point::new(x, y), Size::new(w, h));
//     let colors = bytes.chunks_exact(2).map(|b| {
//         let v = u16::from_be_bytes([b[0], b[1]]);
//         Rgb565::from(RawU16::new(v))
//     });
//     let _ = disp.fill_contiguous(&area, colors);
// }


// Draw from already-decompressed bytes (used by cache on OLED)
fn draw_image_bytes(
    disp: &mut impl PanelRgb565,
    bytes: &[u8],
    w: u32,
    h: u32,
    clear: bool,
){
    if clear { let _ = disp.clear(Rgb565::BLACK); }
    if bytes.len() != (w * h * 2) as usize { return; }
    let x = (RESOLUTION.saturating_sub(w)) as i32 / 2;
    let y = (RESOLUTION.saturating_sub(h)) as i32 / 2;

    #[cfg(feature = "esp32s3-disp143Oled")]
    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::co5300::DisplayType<'static>>() {
        let _ = co.blit_rect_be_fast(x as u16, y as u16, w as u16, h as u16, bytes);
        return;
    }

    // Generic fallback
    let raw = ImageRawBE::<Rgb565>::new(bytes, w);
    let _ = Image::new(&raw, Point::new(x, y)).draw(disp);
}


#[cfg(feature = "esp32s3-disp143Oled")]
static IMAGE_CACHE: Mutex<RefCell<[Option<(OmnitrixState, Vec<u8>)>; CACHE_CAP]>> =
    Mutex::new(RefCell::new([None, None, None]));


// Helper: select the compressed asset by state
#[cfg(feature = "esp32s3-disp143Oled")]
fn asset_for(state: OmnitrixState) -> &'static [u8] {
    match state {
        OmnitrixState::Alien1  => ALIEN1_IMAGE,
        OmnitrixState::Alien2  => ALIEN2_IMAGE,
        OmnitrixState::Alien3  => ALIEN3_IMAGE,
        OmnitrixState::Alien4  => ALIEN4_IMAGE,
        OmnitrixState::Alien5  => ALIEN5_IMAGE,
        OmnitrixState::Alien6  => ALIEN6_IMAGE,
        OmnitrixState::Alien7  => ALIEN7_IMAGE,
        OmnitrixState::Alien8  => ALIEN8_IMAGE,
        OmnitrixState::Alien9  => ALIEN9_IMAGE,
        OmnitrixState::Alien10 => ALIEN10_IMAGE,
    }
}

#[cfg(feature = "esp32s3-disp143Oled")]
fn omni_next(s: OmnitrixState) -> OmnitrixState {
    match s {
        OmnitrixState::Alien1  => OmnitrixState::Alien2,
        OmnitrixState::Alien2  => OmnitrixState::Alien3,
        OmnitrixState::Alien3  => OmnitrixState::Alien4,
        OmnitrixState::Alien4  => OmnitrixState::Alien5,
        OmnitrixState::Alien5  => OmnitrixState::Alien6,
        OmnitrixState::Alien6  => OmnitrixState::Alien7,
        OmnitrixState::Alien7  => OmnitrixState::Alien8,
        OmnitrixState::Alien8  => OmnitrixState::Alien9,
        OmnitrixState::Alien9  => OmnitrixState::Alien10,
        OmnitrixState::Alien10 => OmnitrixState::Alien1,
    }
}
#[cfg(feature = "esp32s3-disp143Oled")]
fn omni_prev(s: OmnitrixState) -> OmnitrixState {
    match s {
        OmnitrixState::Alien1  => OmnitrixState::Alien10,
        OmnitrixState::Alien2  => OmnitrixState::Alien1,
        OmnitrixState::Alien3  => OmnitrixState::Alien2,
        OmnitrixState::Alien4  => OmnitrixState::Alien3,
        OmnitrixState::Alien5  => OmnitrixState::Alien4,
        OmnitrixState::Alien6  => OmnitrixState::Alien5,
        OmnitrixState::Alien7  => OmnitrixState::Alien6,
        OmnitrixState::Alien8  => OmnitrixState::Alien7,
        OmnitrixState::Alien9  => OmnitrixState::Alien8,
        OmnitrixState::Alien10 => OmnitrixState::Alien9,
    }
}

// LRU helpers
#[cfg(feature = "esp32s3-disp143Oled")]
fn cache_take(state: OmnitrixState) -> Option<Vec<u8>> {
    critical_section::with(|cs| {
        let mut arr = IMAGE_CACHE.borrow(cs).borrow_mut();
        if let Some(idx) =
            (0..CACHE_CAP).find(|&i| arr[i].as_ref().map(|(s, _)| *s) == Some(state))
        {
            let (_, buf) = arr[idx].take().unwrap();
            // Shift newer entries forward (maintain recency)
            for j in (0..idx).rev() {
                arr[j + 1] = arr[j].take();
            }
            return Some(buf);
        }
        None
    })
}

#[cfg(feature = "esp32s3-disp143Oled")]
fn cache_put(state: OmnitrixState, buf: Vec<u8>) {
    if buf.len() != (IMG_W * IMG_H * 2) as usize {
        return;
    }
    critical_section::with(|cs| {
        let mut arr = IMAGE_CACHE.borrow(cs).borrow_mut();
        // Remove existing same key if present
        if let Some(idx) =
            (0..CACHE_CAP).find(|&i| arr[i].as_ref().map(|(s, _)| *s) == Some(state))
        {
            arr[idx] = None;
        } else {
            // Evict LRU (last)
            arr[CACHE_CAP - 1] = None;
        }
        // Shift right and insert as MRU at index 0
        for j in (0..CACHE_CAP - 1).rev() {
            arr[j + 1] = arr[j].take();
        }
        arr[0] = Some((state, buf));
    });
}

#[cfg(feature = "esp32s3-disp143Oled")]
fn cache_contains(state: OmnitrixState) -> bool {
    critical_section::with(|cs| {
        IMAGE_CACHE
            .borrow(cs)
            .borrow()
            .iter()
            .any(|e| e.as_ref().map(|(s, _)| *s) == Some(state))
    })
}

// Prefetch up to two neighbors after drawing (amortizes cost)
#[cfg(feature = "esp32s3-disp143Oled")]
fn prefetch_neighbors(current: OmnitrixState) {
    let n = omni_next(current);
    let p = omni_prev(current);

    // Prefetch next if missing
    if !cache_contains(n) {
        let z = asset_for(n);
        let bytes = decompress_to_vec_zlib(z).unwrap_or_default();
        if bytes.len() == (IMG_W * IMG_H * 2) as usize {
            cache_put(n, bytes);
        }
    }
    // Prefetch prev if missing
    if !cache_contains(p) {
        let z = asset_for(p);
        let bytes = decompress_to_vec_zlib(z).unwrap_or_default();
        if bytes.len() == (IMG_W * IMG_H * 2) as usize {
            cache_put(p, bytes);
        }
    }
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
                let diameter: u32 = RESOLUTION / 2;
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
            #[cfg(feature = "esp32s3-disp143Oled")]
            {
                // Take from cache or decompress once
                let bytes = cache_take(omnitrix_state).unwrap_or_else(|| {
                    let b = decompress_to_vec_zlib(image).unwrap_or_default();
                    b
                });
                draw_image_bytes(disp, &bytes, IMG_W, IMG_H, false);
                // Put back as most-recent
                cache_put(omnitrix_state, bytes);
                // Warm both neighbors
                prefetch_neighbors(omnitrix_state);
            }
            #[cfg(feature = "devkit-esp32s3-disp128")]
            {
                draw_image_runtime(disp, image, IMG_W, IMG_H, false);
            }
        }
        
        Page::Info => {
            draw_text(disp, "Info Screen", Rgb565::CYAN, Rgb565::BLACK, CENTER, CENTER, true);
            // draw_image(disp, MY_IMAGE, IMG_W, IMG_H, false);
        }
    }
}