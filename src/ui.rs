//! UI state management and display rendering module.
//!
//! This module provides:
//! - The `UiState` enum and its navigation methods (`next`, `prev`, etc.)
//! - The `update_ui` function to render the current UI state to the display
//! - Drawing helpers for text, shapes, and layout
//!
//! Designed for use with embedded-graphics, mipidsi, and ESP-HAL display drivers.
//! All drawing is centered on a 240x240 display, but can be adapted for other sizes.

extern crate alloc;
use alloc::{
    boxed::Box,
    vec::Vec,
    vec,
};
use core::cell::RefCell;
use critical_section::Mutex;
use core::slice;

use esp_backtrace as _;

// ESP-HAL imports
// use esp_hal::{
//     gpio::Output,
//     spi::master::Spi,
//     Blocking,
// };

// Embedded-graphics
use embedded_graphics::{
    Drawable, draw_target::DrawTarget, image::{Image, ImageRaw, ImageRawBE}, mono_font::{MonoTextStyle, MonoTextStyleBuilder, ascii::{FONT_6X10, FONT_10X20}}, pixelcolor::Rgb565, prelude::{OriginDimensions, Point, Primitive, RgbColor, Size, IntoStorage}, primitives::{Circle, PrimitiveStyle, Rectangle, Triangle}, text::{Alignment, Baseline, Text}
};

use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

use core::any::Any;

// Make a lightweight trait bound we’ll use for the factory’s return type.
pub trait PanelRgb565: DrawTarget<Color = Rgb565> + OriginDimensions + Any {}
impl<T> PanelRgb565 for T where T: DrawTarget<Color = Rgb565> + OriginDimensions + Any {}


// Display configuration, (0,0) is top-left corner

pub const RESOLUTION: u32 = 466;

pub const CENTER: i32 = (RESOLUTION / 2) as i32;

// Feature-selected image dimensions (adjust OLED to 466 if you have 466×466 assets)

pub const IMG_W: u32 = 466; // change to 466 if you add 466×466 assets
pub const IMG_H: u32 = 466; // change to 466 if you add 466×466 assets


// Compile-time suffix for asset filenames
macro_rules! res { () => { "466x466" } } // set to "466x466" when you have OLED-sized assets

const OMNI_LIME: Rgb565 = Rgb565::new(0x11, 0x38, 0x01); // #8BE308

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

// Hourglass buffer for decompression
static HOURGLASS_BUF: Mutex<RefCell<Option<Box<[u8]>>>> = Mutex::new(RefCell::new(None));

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

    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::co5300::DisplayType<'static>>() {
        // Temporarily enable even-alignment for the controller's fast blit
        co.set_align_even(true);
        let _ = co.blit_rect_be_fast(x as u16, y as u16, w as u16, h as u16, bytes);
        co.set_align_even(false);
        return;
    }
}

// Size of one image in bytes
const SLOT_BYTES: usize = (IMG_W as usize) * (IMG_H as usize) * 2;
// Global arena: use raw pointer + len to avoid RefCell borrow lifetime issues
static mut IMAGE_ARENA_PTR: *mut u8 = core::ptr::null_mut();
static mut IMAGE_ARENA_LEN: usize = 0;

static ARENA_FILLED: Mutex<RefCell<[bool; 10]>> =
    Mutex::new(RefCell::new([false; 10]));

// Allocate one big contiguous arena for up to `count` images.
// Returns how many slots actually fit (<= count).
pub fn init_image_arena(count: usize) -> usize {
    let want = count.min(10);
    // Try to reserve total bytes without panicking
    let mut v = Vec::<u8>::new();
    let mut ok_slots = want;
    while ok_slots > 0 {
        let need = SLOT_BYTES * ok_slots;
        if v.try_reserve_exact(need).is_ok() {
            v.resize(need, 0);
            let leaked: &'static mut [u8] = alloc::boxed::Box::leak(v.into_boxed_slice());
            unsafe {
                IMAGE_ARENA_PTR = leaked.as_mut_ptr();
                IMAGE_ARENA_LEN = leaked.len();
            }
            // Mark slots empty
            critical_section::with(|cs| {
                *ARENA_FILLED.borrow(cs).borrow_mut() = [false; 10];
            });
            return ok_slots;
        }
        ok_slots -= 1;
    }
    0
}

// Mark/Check a slot as filled
fn set_filled(idx: usize) {
    critical_section::with(|cs| ARENA_FILLED.borrow(cs).borrow_mut()[idx] = true);
}
fn is_filled(idx: usize) -> bool {
    critical_section::with(|cs| ARENA_FILLED.borrow(cs).borrow()[idx])
}

// Write bytes into a slot (copy). Returns false if arena not ready.
fn write_slot(idx: usize, src: &[u8]) -> bool {
    if src.len() != SLOT_BYTES { return false; }
    let start = idx * SLOT_BYTES;
    unsafe {
        if IMAGE_ARENA_PTR.is_null() || start + SLOT_BYTES > IMAGE_ARENA_LEN {
            return false;
        }
        core::ptr::copy_nonoverlapping(src.as_ptr(), IMAGE_ARENA_PTR.add(start), SLOT_BYTES);
    }
    set_filled(idx);
    true
}

// Decompress one image into its arena slot (bounded). Returns true on success.
pub fn cache_slot(idx: usize) -> bool {
    if idx >= 10 || is_filled(idx) { return true; }
    let state = match idx {
        0 => OmnitrixState::Alien1, 1 => OmnitrixState::Alien2, 2 => OmnitrixState::Alien3,
        3 => OmnitrixState::Alien4, 4 => OmnitrixState::Alien5, 5 => OmnitrixState::Alien6,
        6 => OmnitrixState::Alien7, 7 => OmnitrixState::Alien8, 8 => OmnitrixState::Alien9,
        _ => OmnitrixState::Alien10,
    };
    let z = asset_for(state);
    let tmp = decompress_to_vec_zlib_with_limit(z, SLOT_BYTES)
        .unwrap_or_default();
    if tmp.len() != SLOT_BYTES { return false; }
    write_slot(idx, &tmp)
}

// Get a read-only slice for a cached image; None if not cached
fn get_cached_slice(state: OmnitrixState) -> Option<&'static [u8]> {
    let idx = omni_index(state);
    if !is_filled(idx) { return None; }
    let start = idx * SLOT_BYTES;
    unsafe {
        if IMAGE_ARENA_PTR.is_null() || start + SLOT_BYTES > IMAGE_ARENA_LEN {
            return None;
        }
        Some(slice::from_raw_parts(IMAGE_ARENA_PTR.add(start), SLOT_BYTES))
    }
}

// Map OmnitrixState to a stable index 0..9
fn omni_index(s: OmnitrixState) -> usize {
    match s {
        OmnitrixState::Alien1  => 0,
        OmnitrixState::Alien2  => 1,
        OmnitrixState::Alien3  => 2,
        OmnitrixState::Alien4  => 3,
        OmnitrixState::Alien5  => 4,
        OmnitrixState::Alien6  => 5,
        OmnitrixState::Alien7  => 6,
        OmnitrixState::Alien8  => 7,
        OmnitrixState::Alien9  => 8,
        OmnitrixState::Alien10 => 9,
    }
}

// Get the compressed asset bytes for a given OmnitrixState
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

// Cache a full-frame Omnitrix-style hourglass into a static buffer.
pub fn cache_hourglass_logo(color: Rgb565, bg: Rgb565) {
    let size = RESOLUTION as usize;
    let center = size / 2;

    // This is a magic number, calculated from the original image this drawing is based on
    let waist = 148;
    let drop = (size - waist) as f32;

    // Prepare buffer: RGB565 big-endian, size*size*2 bytes
    let mut buf = alloc::vec![0u8; size * size * 2];
    let fg = color.into_storage().to_be_bytes();
    let bgc = bg.into_storage().to_be_bytes();

    // Draw hourglass into buffer
    for y in 0..size {

        // Compute width at this y (float math for smooth edges)
        let width = if y < center {
            size as f32 - drop * (y as f32 / center as f32)
        } else {
            waist as f32 + drop * ((y - center) as f32 / center as f32)
        };

        // Compute width at this y (float math for smooth edges)
        let width = (width + 0.5) as usize;
        let left = ((size - width) / 2).max(0);
        let right = (left + width).min(size);

        // Fill pixels
        for x in 0..size {
            let off = (y * size + x) * 2;
            let px = if x >= left && x < right { fg } else { bgc };
            buf[off] = px[0];
            buf[off + 1] = px[1];
        }
    }

    // Store boxed slice in static mutex
    let boxed = buf.into_boxed_slice();
    critical_section::with(|cs| {
        *HOURGLASS_BUF.borrow(cs).borrow_mut() = Some(boxed);
    });
}

// Retrieve the cached hourglass logo buffer, if available.
fn get_hourglass_logo() -> Option<Box<[u8]>> {
    critical_section::with(|cs| {
        HOURGLASS_BUF.borrow(cs).borrow().as_ref().map(|b| b.clone())
    })
}

pub fn draw_hourglass_logo(
    disp: &mut impl PanelRgb565,
    color: Rgb565,
    bg: Rgb565,
    clear: bool,
) {
    if clear {
        // Clear the display with background color
        let _ = disp.clear(Rgb565::BLACK);
    }
    let size = RESOLUTION as usize;
    let center = size / 2;
    // This is a magic number, calculated from the original image this drawing is based on
    let waist = 80;
    let drop = (size - waist) as f32;

    // Prepare buffer: RGB565 big-endian, size*size*2 bytes
    let mut buf = vec![0u8; size * size * 2];

    // Precompute color bytes
    let fg = color.into_storage().to_be_bytes();
    let bgc = bg.into_storage().to_be_bytes();

    for y in 0..size {
        // Compute width at this y (float math for smooth edges)
        let width = if y < center {
            size as f32 - drop * (y as f32 / center as f32)
        } else {
            waist as f32 + drop * ((y - center) as f32 / center as f32)
        };

        // Compute width at this y (float math for smooth edges)
        let width = (width + 0.5) as usize;
        let left = ((size - width) / 2).max(0);
        let right = (left + width).min(size);

        // Fill pixels
        for x in 0..size {
            let off = (y * size + x) * 2;
            let px = if x >= left && x < right { fg } else { bgc };
            buf[off] = px[0];
            buf[off + 1] = px[1];
        }
    }
    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::co5300::DisplayType<'static>>() {
        // Temporarily enable even-alignment for the controller's fast blit
        // co.set_align_even(true);
        let _ = co.blit_rect_be_fast(0, 0, RESOLUTION as u16, RESOLUTION as u16, &buf);
        // co.set_align_even(false);
        return;
    }
    let raw = ImageRawBE::<Rgb565>::new(&buf, size as u32);
    let _ = Image::new(&raw, Point::new(0, 0)).draw(disp);
}


fn draw_diamond_bg(disp: &mut impl PanelRgb565, color: Rgb565, clear_bg: bool) {
    if clear_bg { let _ = disp.clear(Rgb565::BLACK); }
    let size = RESOLUTION as i32;
    let cy = size / 2;

    // Draw diamond with scanline spans: width grows to center, then shrinks.
    for y in 0..size {
        let up = if y <= cy { y } else { size - 1 - y };
        let width = (up * 2 + 1).clamp(1, size); // odd widths look centered
        let left = ((size - width) / 2).max(0);
        let _ = Rectangle::new(
            Point::new(left, y),
            Size::new(width as u32, 1),
        )
        .into_styled(embedded_graphics::primitives::PrimitiveStyle::with_fill(color))
        .draw(disp);
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
                disp.clear(OMNI_LIME).ok();
            }
        }
        return;
    }

    match state.page {
        Page::Main(menu_state) => {
            match menu_state {
                MainMenuState::Home => {
                    // if let Some(buf) = get_hourglass_logo() {
                    //     draw_image_bytes(disp, &buf, RESOLUTION, RESOLUTION, false);
                    // } else {
                    //     // Fallback if not cached
                    //     cache_hourglass_logo(OMNI_LIME, Rgb565::BLACK);
                    //     if let Some(buf) = get_hourglass_logo() {
                    //         draw_image_bytes(disp, &buf, RESOLUTION, RESOLUTION, false);
                    //     }
                    // }
                    draw_hourglass_logo(disp, OMNI_LIME, Rgb565::BLACK, false);
                }
                MainMenuState::Start => {
                    draw_text(disp, "Main: Start", Rgb565::WHITE, Rgb565::GREEN, CENTER, CENTER, true);
                }
                MainMenuState::About => {
                    draw_text(disp, "Main: About", Rgb565::WHITE, Rgb565::GREEN, CENTER, CENTER, true);
                }
            }
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
            if let Some(bytes) = get_cached_slice(omnitrix_state) {
                draw_image_bytes(disp, bytes, IMG_W, IMG_H, false);
            } else {
                // Fallback if precache failed
                let z = asset_for(omnitrix_state);
                let tmp = decompress_to_vec_zlib_with_limit(z, SLOT_BYTES)
                    .unwrap_or_default();
                // Draw if valid
                if tmp.len() == SLOT_BYTES {
                    draw_image_bytes(disp, &tmp, IMG_W, IMG_H, false);
                    // Store into arena (copy) to avoid re-decompress
                    let _ = write_slot(omni_index(omnitrix_state), &tmp);
                }
            }
        }
        
        Page::Info => {
            draw_text(disp, "Info Screen", Rgb565::CYAN, Rgb565::BLACK, CENTER, CENTER, true);
            // let lime = Rgb565::new(0x11, 0x38, 0x01); // #8BE308
            // draw_hourglass_logo(disp, lime, Rgb565::BLACK, true);
            // draw_image(disp, MY_IMAGE, IMG_W, IMG_H, false);
        }
    }
}