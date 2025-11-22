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
    Drawable, 
    draw_target::DrawTarget, 
    image::{Image, ImageRaw, ImageRawBE}, 
    mono_font::{MonoTextStyle, MonoTextStyleBuilder, 
    ascii::{FONT_6X10, FONT_10X20}}, 
    pixelcolor::Rgb565, 
    prelude::{OriginDimensions, Point, Primitive, RgbColor, Size, IntoStorage}, 
    primitives::{Circle, PrimitiveStyle, Rectangle, Triangle}, 
    text::{Alignment, Baseline, Text}
};

use core::any::Any;

// Make a lightweight trait bound we’ll use for the factory’s return type.
pub trait PanelRgb565: DrawTarget<Color = Rgb565> + OriginDimensions + Any {}
impl<T> PanelRgb565 for T where T: DrawTarget<Color = Rgb565> + OriginDimensions + Any {}


// Display configuration, (0,0) is top-left corner

pub const RESOLUTION: u32 = 466;

pub const CENTER: i32 = (RESOLUTION / 2) as i32;

// Feature-selected image dimensions (adjust OLED to 466 if you have 466×466 assets)

pub const MAX_IMG_W: u32 = 466; 
pub const MAX_IMG_H: u32 = 466;

pub const IMG_W: u32 = 308;
pub const IMG_H: u32 = 374;


// Compile-time suffix for asset filenames
macro_rules! res { () => { "308x374" } } // set to "308x374" when you have OLED-sized assets

const OMNI_LIME: Rgb565 = Rgb565::new(0x11, 0x38, 0x01); // #8BE308

// Feature-picked assets
static ALIEN1_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien1_",  res!(), "_rgb565_be.raw"));
static ALIEN2_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien2_",  res!(), "_rgb565_be.raw"));
static ALIEN3_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien3_",  res!(), "_rgb565_be.raw"));
static ALIEN4_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien4_",  res!(), "_rgb565_be.raw"));
static ALIEN5_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien5_",  res!(), "_rgb565_be.raw"));
static ALIEN6_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien6_",  res!(), "_rgb565_be.raw"));
static ALIEN7_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien7_",  res!(), "_rgb565_be.raw"));
static ALIEN8_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien8_",  res!(), "_rgb565_be.raw"));
static ALIEN9_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien9_",  res!(), "_rgb565_be.raw"));
static ALIEN10_IMAGE: &[u8] = include_bytes!(concat!("assets/alien10_", res!(), "_rgb565_be.raw"));
static ALIEN_LOGO: &[u8]    = include_bytes!(concat!("assets/omnitrix_logo_466x466_rgb565_be.raw"));

static LOGO_CACHE: Mutex<RefCell<Option<&'static [u8]>>> = Mutex::new(RefCell::new(None));
// compressed assets
// use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;
// static ALIEN1_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien1_",  res!(), "_rgb565_be.raw.zlib"));
// static ALIEN2_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien2_",  res!(), "_rgb565_be.raw.zlib"));
// static ALIEN3_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien3_",  res!(), "_rgb565_be.raw.zlib"));
// static ALIEN4_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien4_",  res!(), "_rgb565_be.raw.zlib"));
// static ALIEN5_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien5_",  res!(), "_rgb565_be.raw.zlib"));
// static ALIEN6_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien6_",  res!(), "_rgb565_be.raw.zlib"));
// static ALIEN7_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien7_",  res!(), "_rgb565_be.raw.zlib"));
// static ALIEN8_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien8_",  res!(), "_rgb565_be.raw.zlib"));
// static ALIEN9_IMAGE: &[u8]  = include_bytes!(concat!("assets/alien9_",  res!(), "_rgb565_be.raw.zlib"));
// static ALIEN10_IMAGE: &[u8] = include_bytes!(concat!("assets/alien10_", res!(), "_rgb565_be.raw.zlib"));

// Hourglass buffer for decompression
static HOURGLASS_BUF: Mutex<RefCell<Option<Box<[u8]>>>> = Mutex::new(RefCell::new(None));

// Page kind tracker for optimization
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PageKind { Main, Settings, Omnitrix, Info }

static LAST_PAGE_KIND: Mutex<RefCell<Option<PageKind>>> = Mutex::new(RefCell::new(None));

static LAST_OMNI_TRANSFORM_ACTIVE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

// Navigation history management
static NAV_HISTORY: Mutex<RefCell<Vec<Page>>> = Mutex::new(RefCell::new(Vec::new()));
fn nav_push(p: Page) {
    critical_section::with(|cs| {
        NAV_HISTORY.borrow(cs).borrow_mut().push(p);
    });
}
fn nav_pop() -> Option<Page> {
    critical_section::with(|cs| NAV_HISTORY.borrow(cs).borrow_mut().pop())
}

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
    Home,          // just show home
    SettingsApp,   // enter Settings
    InfoApp,       // enter Info
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
    // Move to the next item/state in the current layer (rotary CW)
    pub fn next_item(self) -> Self {
        if self.dialog.is_some() { return self; }
        let next_page = match self.page {
            Page::Main(state) => {
                let next = match state {
                    MainMenuState::Home        => MainMenuState::SettingsApp,
                    MainMenuState::SettingsApp => MainMenuState::InfoApp,
                    MainMenuState::InfoApp     => MainMenuState::Home,
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

    // Move to the previous item/state (rotary CCW)
    pub fn prev_item(self) -> Self {
        if self.dialog.is_some() { return self; }
        let prev_page = match self.page {
            Page::Main(state) => {
                let prev = match state {
                    MainMenuState::Home        => MainMenuState::InfoApp,
                    MainMenuState::SettingsApp => MainMenuState::Home,
                    MainMenuState::InfoApp     => MainMenuState::SettingsApp,
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

    // Go back (Button 1)
    pub fn back(self) -> Self {
        if self.dialog.is_some() {
            return Self { page: self.page, dialog: None };
        }
        if let Some(prev) = nav_pop() {
            return Self { page: prev, dialog: None };
        }
        // Fallback if no history
        Self { page: Page::Main(MainMenuState::Home), dialog: None }
    }

    // Select/enter (Button 2) 
    pub fn select(self) -> Self {
        if let Some(_) = self.dialog {
            return Self { page: self.page, dialog: None };
        }
        match self.page {
            Page::Main(state) => {
                nav_push(Page::Main(state));
                let page = match state {
                    MainMenuState::Home        => Page::Omnitrix(OmnitrixState::Alien1),
                    MainMenuState::SettingsApp => Page::Settings(SettingsMenuState::Volume),
                    MainMenuState::InfoApp     => Page::Info,
                };
                Self { page, dialog: None }
            }
            Page::Settings(_) => Self { page: self.page, dialog: None },
            Page::Omnitrix(_) => Self { page: self.page, dialog: None }, // changed
            Page::Info => Self { page: self.page, dialog: None },
        }
    }

    // Omnitrix transform (Button 3)
    pub fn transform(self) -> Self {
        // Only if on Omnitrix and no dialog already
        if matches!(self.page, Page::Omnitrix(_)) && self.dialog.is_none() {
            Self { page: self.page, dialog: Some(Dialog::TransformPage) }
        } else {
            self
        }
    }

}

// helper function to draw centered text
fn draw_text(
    disp: &mut impl PanelRgb565,
    text: &str,
    fg: Rgb565,
    bg: Option<Rgb565>,
    x_point: i32,
    y_point: i32,
    clear: bool,
    update_fb: bool,
) {
    if clear {
        // Prefer no-FB clear if available and requested
        if !update_fb {
            if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>() {
                let _ = co.fill_rect_solid_no_fb(0, 0, RESOLUTION as u16, RESOLUTION as u16, Rgb565::BLACK);
            } else {
                let _ = disp.clear(Rgb565::BLACK);
            }
        } else {
            let _ = disp.clear(Rgb565::BLACK);
        }
    }
    let mut builder = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(fg);
    if let Some(b) = bg {
        builder = builder.background_color(b);
    }
    let style = builder.build();
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
pub fn draw_image_bytes(
    disp: &mut impl PanelRgb565,
    bytes: &[u8],
    w: u32,
    h: u32,
    clear: bool,
    update_fb: bool,
){
    // Clear background if requested
    if clear {
        if !update_fb {
            if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>() {
                let _ = co.fill_rect_solid_no_fb(0, 0, RESOLUTION as u16, RESOLUTION as u16, Rgb565::BLACK);
            } else {
                let _ = disp.clear(Rgb565::BLACK);
            }
        } else {
            let _ = disp.clear(Rgb565::BLACK);
        }
    }
    // Validate size
    if bytes.len() != (w * h * 2) as usize { return; }
    let x = (RESOLUTION.saturating_sub(w)) as i32 / 2;
    let y = (RESOLUTION.saturating_sub(h)) as i32 / 2;

    // Try fast raw blit if this really is the CO5300 driver (DMA or non-DMA alias).
    // The display backend re-exports its concrete type as display::DisplayType.
    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>() {
        let res = if update_fb {
            co.blit_rect_be_fast(x as u16, y as u16, w as u16, h as u16, bytes)
        } else {
            co.blit_rect_be_fast_no_fb(x as u16, y as u16, w as u16, h as u16, bytes)
        };
        if let Err(e) = res {
            esp_println::println!("fast blit failed: {:?}; fallback", e);
            let raw = ImageRawBE::<Rgb565>::new(bytes, w);
            let _ = Image::new(&raw, Point::new(x, y)).draw(disp);
        }
    } else {
        let raw = ImageRawBE::<Rgb565>::new(bytes, w);
        let _ = Image::new(&raw, Point::new(x, y)).draw(disp);
    }
}


const OMNI_MAX: usize = 10;

#[derive(Copy, Clone)]
struct ImgMeta { w: u32, h: u32 }

// Dimensions table; adjust per asset if not 466x466
fn omni_dims(s: OmnitrixState) -> ImgMeta {
    match s {
        // Example: uncomment and adjust if an asset is 308x374
        OmnitrixState::Alien1 => ImgMeta { w: 308, h: 374 },
        OmnitrixState::Alien2 => ImgMeta { w: 308, h: 374 },
        OmnitrixState::Alien3 => ImgMeta { w: 308, h: 374 },
        OmnitrixState::Alien4 => ImgMeta { w: 308, h: 374 },
        OmnitrixState::Alien5 => ImgMeta { w: 308, h: 374 },
        OmnitrixState::Alien6 => ImgMeta { w: 308, h: 374 },
        OmnitrixState::Alien7 => ImgMeta { w: 308, h: 374 },
        OmnitrixState::Alien8 => ImgMeta { w: 308, h: 374 },
        OmnitrixState::Alien9 => ImgMeta { w: 308, h: 374 },
        OmnitrixState::Alien10 => ImgMeta { w: 308, h: 374 },
        _ => ImgMeta { w: MAX_IMG_W, h: MAX_IMG_H },
    }
}

static OMNI_BYTES: Mutex<RefCell<[Option<&'static [u8]>; OMNI_MAX]>> =
    Mutex::new(RefCell::new([None; OMNI_MAX])); // Cached image byte slices

static OMNI_META: Mutex<RefCell<[ImgMeta; OMNI_MAX]>> =
    Mutex::new(RefCell::new([ImgMeta { w: IMG_W, h: IMG_H }; OMNI_MAX])); // Cached image metadata

// Pre-cache one state; returns true on success
pub fn precache_one(s: OmnitrixState) -> bool {
    let idx = omni_index(s);
    // Already cached?
    if critical_section::with(|cs| OMNI_BYTES.borrow(cs).borrow()[idx].is_some()) {
        return true;
    }
    let meta = omni_dims(s);
    let need = (meta.w as usize) * (meta.h as usize) * 2;
    let src = asset_for(s);

    // If the asset is already raw RGB565 of the right size, copy it to PSRAM.
    if src.len() == need {
        let mut v = alloc::vec![0u8; need];
        v.copy_from_slice(src);
        let leaked: &'static mut [u8] = alloc::boxed::Box::leak(v.into_boxed_slice());
        critical_section::with(|cs| {
            OMNI_BYTES.borrow(cs).borrow_mut()[idx] = Some(leaked as &'static [u8]);
            OMNI_META.borrow(cs).borrow_mut()[idx] = meta;
        });
        return true;
    }

    // Otherwise, try to decompress as zlib into the exact size.
    if let Ok(tmp) = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(src, need) {
        if tmp.len() == need {
            let leaked: &'static mut [u8] = alloc::boxed::Box::leak(tmp.into_boxed_slice());
            critical_section::with(|cs| {
                OMNI_BYTES.borrow(cs).borrow_mut()[idx] = Some(leaked as &'static [u8]);
                OMNI_META.borrow(cs).borrow_mut()[idx] = meta;
            });
            return true;
        }
    }

    false
}

// Pre-cache all (call once at boot)
pub fn precache_all() -> usize {
    let mut ok = 0;
    for s in [
        OmnitrixState::Alien1, OmnitrixState::Alien2, OmnitrixState::Alien3,
        OmnitrixState::Alien4, OmnitrixState::Alien5, OmnitrixState::Alien6,
        OmnitrixState::Alien7, OmnitrixState::Alien8, OmnitrixState::Alien9,
        OmnitrixState::Alien10,
    ] {
        if precache_one(s) { ok += 1; } else { break; }
    }
    ok
}

// Cache the Omnitrix logo (466x466) once.
pub fn precache_logo() -> bool {
    let need = 466usize * 466usize * 2;
    if critical_section::with(|cs| LOGO_CACHE.borrow(cs).borrow().is_some()) {
        return true;
    }
    if ALIEN_LOGO.len() != need { return false; }
    let mut v = alloc::vec![0u8; need];
    v.copy_from_slice(ALIEN_LOGO);
    let leaked: &'static mut [u8] = alloc::boxed::Box::leak(v.into_boxed_slice());
    critical_section::with(|cs| {
        LOGO_CACHE.borrow(cs).replace(Some(leaked as &'static [u8]));
    });
    true
}

pub fn get_logo_image() -> Option<&'static [u8]> {
    critical_section::with(|cs| LOGO_CACHE.borrow(cs).borrow().clone())
}

// Get cached bytes and dims
fn get_cached_image(s: OmnitrixState) -> Option<(&'static [u8], u32, u32)> {
    let idx = omni_index(s);
    critical_section::with(|cs| {
        let b = OMNI_BYTES.borrow(cs).borrow()[idx]?;
        let m = OMNI_META.borrow(cs).borrow()[idx];
        Some((b, m.w, m.h))
    })
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

// pub fn draw_hourglass_logo(
//     disp: &mut impl PanelRgb565,
//     color: Rgb565,
//     bg: Rgb565,
//     clear: bool,
// ) {
//     if clear {
//         // Clear the display with background color
//         let _ = disp.clear(Rgb565::BLACK);
//     }
//     let size = RESOLUTION as usize;
//     let center = size / 2;
//     // This is a magic number, calculated from the original image this drawing is based on
//     let waist = 80;
//     let drop = (size - waist) as f32;

//     // Prepare buffer: RGB565 big-endian, size*size*2 bytes
//     let mut buf = vec![0u8; size * size * 2];

//     // Precompute color bytes
//     let fg = color.into_storage().to_be_bytes();
//     let bgc = bg.into_storage().to_be_bytes();

//     for y in 0..size {
//         // Compute width at this y (float math for smooth edges)
//         let width = if y < center {
//             size as f32 - drop * (y as f32 / center as f32)
//         } else {
//             waist as f32 + drop * ((y - center) as f32 / center as f32)
//         };

//         // Compute width at this y (float math for smooth edges)
//         let width = (width + 0.5) as usize;
//         let left = ((size - width) / 2).max(0);
//         let right = (left + width).min(size);

//         // Fill pixels
//         for x in 0..size {
//             let off = (y * size + x) * 2;
//             let px = if x >= left && x < right { fg } else { bgc };
//             buf[off] = px[0];
//             buf[off + 1] = px[1];
//         }
//     }
//     if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>() {
//         let _ = co.blit_rect_be_fast(0, 0, RESOLUTION as u16, RESOLUTION as u16, &buf);
//         return;
//     }
//     let raw = ImageRawBE::<Rgb565>::new(&buf, size as u32);
//     let _ = Image::new(&raw, Point::new(0, 0)).draw(disp);
// }

// helper function to update the display based on UI_STATE
pub fn update_ui(
    disp: &mut impl PanelRgb565,
    state: UiState,
)
{
    // Clear when:
    // - entering Omnitrix from another page, OR
    // - exiting Transform dialog while staying in Omnitrix
    let current_kind = match state.page {
        Page::Main(_)     => PageKind::Main,
        Page::Settings(_) => PageKind::Settings,
        Page::Omnitrix(_) => PageKind::Omnitrix,
        Page::Info        => PageKind::Info,
    };
    let current_transform_active =
        matches!(state.page, Page::Omnitrix(_)) &&
        matches!(state.dialog, Some(Dialog::TransformPage));

    let should_clear = critical_section::with(|cs| {
        let mut last_kind = LAST_PAGE_KIND.borrow(cs).borrow_mut();
        let mut last_tx   = LAST_OMNI_TRANSFORM_ACTIVE.borrow(cs).borrow_mut();

        let entering_omni = current_kind == PageKind::Omnitrix && *last_kind != Some(PageKind::Omnitrix);
        let exiting_transform = (*last_tx) && current_kind == PageKind::Omnitrix && !current_transform_active;

        // update trackers for next frame
        *last_kind = Some(current_kind);
        *last_tx = current_transform_active;

        entering_omni || exiting_transform
    });

    if should_clear {
        let _ = disp.clear(Rgb565::BLACK);
    }

    if let Some(dialog) = state.dialog {
        match dialog {
            Dialog::VolumeAdjust =>
                draw_text(disp, "Adjust Volume (TEMP)", Rgb565::WHITE, Some(Rgb565::RED), CENTER, CENTER, true, true),
            Dialog::BrightnessAdjust =>
                draw_text(disp, "Adjust Brightness (TEMP)", Rgb565::WHITE, Some(Rgb565::MAGENTA), CENTER, CENTER, true, true),
            Dialog::ResetSelector =>
                draw_text(disp, "Reset? (TEMP)", Rgb565::WHITE, Some(Rgb565::YELLOW), CENTER, CENTER, true, true),
            Dialog::HomePage =>
                draw_text(disp, "Home Page (TEMP)", Rgb565::GREEN, Some(Rgb565::BLACK), CENTER, CENTER, true, true),
            Dialog::StartPage =>
                draw_text(disp, "Start Page (TEMP)", Rgb565::BLUE, Some(Rgb565::BLACK), CENTER, CENTER, true, true),
            Dialog::AboutPage =>
                draw_text(disp, "About Page (TEMP)", Rgb565::CYAN, Some(Rgb565::BLACK), CENTER, CENTER, true, true),
            Dialog::TransformPage => {
                // show transform overlay; next frame (when dismissed) will clear due to logic above
                disp.clear(OMNI_LIME).ok();
            }
        }
        return;
    }

    match state.page {
        Page::Main(menu_state) => {
            match menu_state {
                MainMenuState::Home => {
                    // Draw the cached Omnitrix logo asset (no FB mirror)
                    if let Some(buf) = get_logo_image() {
                        draw_image_bytes(disp, buf, 466, 466, false, false);
                    } else {
                        if precache_logo() {
                            if let Some(buf) = get_logo_image() {
                                draw_image_bytes(disp, buf, 466, 466, false, false);
                            }
                        }
                    }
                }
                MainMenuState::SettingsApp => {
                    draw_text(disp, "Settings", Rgb565::WHITE, Some(Rgb565::BLUE), CENTER, CENTER, true, true);
                }
                MainMenuState::InfoApp => {
                    draw_text(disp, "Info", Rgb565::WHITE, Some(Rgb565::CYAN), CENTER, CENTER, true, true);
                }
            }
        }

        Page::Settings(settings_state) => {
            let (msg, fg, bg) = match settings_state {
                SettingsMenuState::Volume     => ("Settings: Volume", Rgb565::YELLOW, Some(Rgb565::BLUE)),
                SettingsMenuState::Brightness => ("Settings: Brightness", Rgb565::YELLOW, Some(Rgb565::BLUE)),
                SettingsMenuState::Reset      => ("Settings: Reset", Rgb565::YELLOW, Some(Rgb565::BLUE)),
            };
            draw_text(disp, msg, fg, bg, CENTER, CENTER, true, true);
        }

        Page::Omnitrix(omnitrix_state) => {
            // Removed per-alien clear; handled by page transition above
            if let Some((bytes, w, h)) = get_cached_image(omnitrix_state) {
                draw_image_bytes(disp, bytes, w, h, false, false);
                // esp_println::println!("Omnitrix: drew cached image");
            } else if precache_one(omnitrix_state) {
                if let Some((bytes, w, h)) = get_cached_image(omnitrix_state) {
                    draw_image_bytes(disp, bytes, w, h, false, false);
                }
            }
        }
        
        Page::Info => {
            disp.clear(Rgb565::WHITE).ok();
            draw_text(disp, "Info Screen", Rgb565::CYAN, None, CENTER, CENTER, false, true);
            // let lime = Rgb565::new(0x11, 0x38, 0x01); // #8BE308
            // draw_hourglass_logo(disp, lime, Rgb565::BLACK, true);
            // draw_image(disp, MY_IMAGE, IMG_W, IMG_H, false);
        }
    }
}
