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
use alloc::vec::Vec;
use core::cell::RefCell;
use critical_section::Mutex;

use esp_backtrace as _;

// Embedded-graphics, a ton are unused but this is a work in progress
use embedded_graphics::{
    draw_target::DrawTarget,
    image::{Image, ImageRaw, ImageRawBE},
    mono_font::{
        ascii::{FONT_10X20, FONT_6X10},
        MonoTextStyle, MonoTextStyleBuilder,
    },
    pixelcolor::Rgb565,
    prelude::{IntoStorage, OriginDimensions, Point, Primitive, RgbColor, Size},
    primitives::{Circle, PrimitiveStyle, Rectangle, Triangle},
    text::{Alignment, Baseline, Text},
    Drawable,
};

use core::any::Any;
use miniz_oxide::inflate::decompress_to_vec_zlib_with_limit;

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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AssetId {
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
    Logo,
    InfoPage,
}

#[derive(Copy, Clone)]
struct AssetSlot {
    data: Option<&'static [u8]>,
    w: u32,
    h: u32,
}

// Number of asset slots
const ASSET_MAX: usize = 12;

macro_rules! res {
    () => {
        "308x374"
    };
} // just a convenience macro for asset paths, a lot have this resolution

// Custom colors
const OMNI_LIME: Rgb565 = Rgb565::new(0x11, 0x38, 0x01); // #8BE308

// Feature-picked assets (compressed, zlib)
static ALIEN1_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien1_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN2_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien2_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN3_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien3_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN4_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien4_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN5_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien5_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN6_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien6_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN7_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien7_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN8_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien8_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN9_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien9_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN10_IMAGE: &[u8] =
    include_bytes!(concat!("assets/alien10_", res!(), "_rgb565_be.raw.zlib"));
static ALIEN_LOGO: &[u8] =
    include_bytes!(concat!("assets/omnitrix_logo_466x466_rgb565_be.raw.zlib"));
static INFO_PAGE_IMAGE: &[u8] =
    include_bytes!(concat!("assets/debug_image_466x466_rgb565_be.raw.zlib"));

// Generic asset cache
static ASSETS: Mutex<RefCell<[AssetSlot; ASSET_MAX]>> = Mutex::new(RefCell::new(
    [AssetSlot {
        data: None,
        w: 0,
        h: 0,
    }; ASSET_MAX],
));

// Page kind tracker for optimization
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum PageKind {
    Main,
    Settings,
    Omnitrix,
    Info,
    Watch,
}
static LAST_PAGE_KIND: Mutex<RefCell<Option<PageKind>>> = Mutex::new(RefCell::new(None));

// Omnitrix transform active tracker
static LAST_OMNI_TRANSFORM_ACTIVE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

// Navigation history management
static NAV_HISTORY: Mutex<RefCell<Vec<Page>>> = Mutex::new(RefCell::new(Vec::new()));

// uses a simple stack for navigation history
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
    Watch(WatchAppState),
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
    Home,        // just show home
    WatchApp,    // enter watch app (analog/digital)
    SettingsApp, // enter Settings
    InfoApp,     // enter Info
}

// States for Watch App
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WatchAppState {
    Analog,
    Digital,
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
        if self.dialog.is_some() {
            return self;
        }
        let next_page = match self.page {
            Page::Main(state) => {
                let next = match state {
                    MainMenuState::Home => MainMenuState::WatchApp,
                    MainMenuState::WatchApp => MainMenuState::SettingsApp,
                    MainMenuState::SettingsApp => MainMenuState::InfoApp,
                    MainMenuState::InfoApp => MainMenuState::Home,
                };
                Page::Main(next)
            }
            Page::Watch(state) => {
                let next = match state {
                    WatchAppState::Analog => WatchAppState::Digital,
                    WatchAppState::Digital => WatchAppState::Analog,
                };
                Page::Watch(next)
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
        Self {
            page: next_page,
            dialog: None,
        }
    }

    // Move to the previous item/state (rotary CCW)
    pub fn prev_item(self) -> Self {
        if self.dialog.is_some() {
            return self;
        }
        let prev_page = match self.page {
            Page::Main(state) => {
                let prev = match state {
                    MainMenuState::Home => MainMenuState::InfoApp,
                    MainMenuState::WatchApp => MainMenuState::Home,
                    MainMenuState::SettingsApp => MainMenuState::WatchApp,
                    MainMenuState::InfoApp => MainMenuState::SettingsApp,
                };
                Page::Main(prev)
            }
            Page::Watch(state) => {
                let prev = match state {
                    WatchAppState::Analog => WatchAppState::Digital,
                    WatchAppState::Digital => WatchAppState::Analog,
                };
                Page::Watch(prev)
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
        Self {
            page: prev_page,
            dialog: None,
        }
    }

    // Go back (Button 1)
    pub fn back(self) -> Self {
        if self.dialog.is_some() {
            return Self {
                page: self.page,
                dialog: None,
            };
        }
        if let Some(prev) = nav_pop() {
            return Self {
                page: prev,
                dialog: None,
            };
        }
        // Fallback if no history
        Self {
            page: Page::Main(MainMenuState::Home),
            dialog: None,
        }
    }

    // Select/enter (Button 2)
    pub fn select(self) -> Self {
        if let Some(_) = self.dialog {
            return Self {
                page: self.page,
                dialog: None,
            };
        }
        match self.page {
            Page::Main(state) => {
                nav_push(Page::Main(state));
                let page = match state {
                    MainMenuState::Home => Page::Omnitrix(OmnitrixState::Alien1),
                    MainMenuState::WatchApp => Page::Watch(WatchAppState::Analog),
                    MainMenuState::SettingsApp => Page::Settings(SettingsMenuState::Volume),
                    MainMenuState::InfoApp => Page::Info,
                };
                Self { page, dialog: None }
            }
            Page::Watch(_) => Self {
                page: self.page,
                dialog: None,
            },
            Page::Settings(_) => Self {
                page: self.page,
                dialog: None,
            },
            Page::Omnitrix(_) => Self {
                page: self.page,
                dialog: None,
            }, // changed
            Page::Info => Self {
                page: self.page,
                dialog: None,
            },
        }
    }

    // Omnitrix transform (Button 3)
    pub fn transform(self) -> Self {
        // Only if on Omnitrix and no dialog already
        if matches!(self.page, Page::Omnitrix(_)) && self.dialog.is_none() {
            Self {
                page: self.page,
                dialog: Some(Dialog::TransformPage),
            }
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
            if let Some(co) =
                (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
            {
                let _ = co.fill_rect_solid_no_fb(
                    0,
                    0,
                    RESOLUTION as u16,
                    RESOLUTION as u16,
                    Rgb565::BLACK,
                );
            } else {
                let _ = disp.clear(Rgb565::BLACK);
            }
        } else {
            let _ = disp.clear(Rgb565::BLACK);
        }
    }
    let mut builder = MonoTextStyleBuilder::new().font(&FONT_10X20).text_color(fg);
    if let Some(b) = bg {
        builder = builder.background_color(b);
    }
    let style = builder.build();
    Text::with_alignment(text, Point::new(x_point, y_point), style, Alignment::Center)
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
) {
    // Clear background if requested
    if clear {
        if !update_fb {
            if let Some(co) =
                (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
            {
                let _ = co.fill_rect_solid_no_fb(
                    0,
                    0,
                    RESOLUTION as u16,
                    RESOLUTION as u16,
                    Rgb565::BLACK,
                );
            } else {
                let _ = disp.clear(Rgb565::BLACK);
            }
        } else {
            let _ = disp.clear(Rgb565::BLACK);
        }
    }
    // Validate size
    if bytes.len() != (w * h * 2) as usize {
        return;
    }
    let x = (RESOLUTION.saturating_sub(w)) as i32 / 2;
    let y = (RESOLUTION.saturating_sub(h)) as i32 / 2;

    // Try fast raw blit if this really is the CO5300 driver (DMA or non-DMA alias).
    // The display backend re-exports its concrete type as display::DisplayType.
    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
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

// Map asset id to cache slot index, dimensions, and compressed blob
fn asset_meta(id: AssetId) -> (usize, u32, u32, &'static [u8]) {
    match id {
        AssetId::Alien1 => (0, 308, 374, ALIEN1_IMAGE),
        AssetId::Alien2 => (1, 308, 374, ALIEN2_IMAGE),
        AssetId::Alien3 => (2, 308, 374, ALIEN3_IMAGE),
        AssetId::Alien4 => (3, 308, 374, ALIEN4_IMAGE),
        AssetId::Alien5 => (4, 308, 374, ALIEN5_IMAGE),
        AssetId::Alien6 => (5, 308, 374, ALIEN6_IMAGE),
        AssetId::Alien7 => (6, 308, 374, ALIEN7_IMAGE),
        AssetId::Alien8 => (7, 308, 374, ALIEN8_IMAGE),
        AssetId::Alien9 => (8, 308, 374, ALIEN9_IMAGE),
        AssetId::Alien10 => (9, 308, 374, ALIEN10_IMAGE),
        AssetId::Logo => (10, 466, 466, ALIEN_LOGO),
        AssetId::InfoPage => (11, 466, 466, INFO_PAGE_IMAGE),
    }
}

fn asset_id_for_state(s: OmnitrixState) -> AssetId {
    match s {
        OmnitrixState::Alien1 => AssetId::Alien1,
        OmnitrixState::Alien2 => AssetId::Alien2,
        OmnitrixState::Alien3 => AssetId::Alien3,
        OmnitrixState::Alien4 => AssetId::Alien4,
        OmnitrixState::Alien5 => AssetId::Alien5,
        OmnitrixState::Alien6 => AssetId::Alien6,
        OmnitrixState::Alien7 => AssetId::Alien7,
        OmnitrixState::Alien8 => AssetId::Alien8,
        OmnitrixState::Alien9 => AssetId::Alien9,
        OmnitrixState::Alien10 => AssetId::Alien10,
    }
}

// Pre-cache a compressed asset into PSRAM
pub fn precache_asset(id: AssetId) -> bool {
    let (idx, w, h, blob) = asset_meta(id);
    let need = (w * h * 2) as usize;
    critical_section::with(|cs| {
        if ASSETS.borrow(cs).borrow()[idx].data.is_some() {
            return true;
        }
        if let Ok(tmp) = decompress_to_vec_zlib_with_limit(blob, need) {
            if tmp.len() == need {
                let leaked: &'static mut [u8] = alloc::boxed::Box::leak(tmp.into_boxed_slice());
                ASSETS.borrow(cs).borrow_mut()[idx] = AssetSlot {
                    data: Some(leaked as &'static [u8]),
                    w,
                    h,
                };
                return true;
            }
        }
        false
    })
}

// Pre-cache all (call once at boot)
pub fn precache_all() -> usize {
    let mut ok = 0;
    for id in [
        AssetId::Alien1,
        AssetId::Alien2,
        AssetId::Alien3,
        AssetId::Alien4,
        AssetId::Alien5,
        AssetId::Alien6,
        AssetId::Alien7,
        AssetId::Alien8,
        AssetId::Alien9,
        AssetId::Alien10,
        AssetId::Logo,
        AssetId::InfoPage,
    ] {
        if precache_asset(id) {
            ok += 1;
        } else {
            break;
        }
    }
    ok
}

// Get cached bytes and dims
pub fn get_cached_asset(id: AssetId) -> Option<(&'static [u8], u32, u32)> {
    let (idx, _, _, _) = asset_meta(id);
    critical_section::with(|cs| {
        let slot = ASSETS.borrow(cs).borrow()[idx];
        slot.data.map(|d| (d, slot.w, slot.h))
    })
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
pub fn update_ui(disp: &mut impl PanelRgb565, state: UiState, redraw: bool) {
    // If caller does not want a redraw this cycle, bail out early.
    if !redraw {
        return;
    }
    // Clear when:
    // - entering Omnitrix from another page, OR
    // - exiting Transform dialog while staying in Omnitrix
    let current_kind = match state.page {
        Page::Main(_) => PageKind::Main,
        Page::Settings(_) => PageKind::Settings,
        Page::Omnitrix(_) => PageKind::Omnitrix,
        Page::Info => PageKind::Info,
        Page::Watch(_) => PageKind::Watch,
    };
    let current_transform_active = matches!(state.page, Page::Omnitrix(_))
        && matches!(state.dialog, Some(Dialog::TransformPage));

    let should_clear_no_fb = critical_section::with(|cs| {
        let mut last_kind = LAST_PAGE_KIND.borrow(cs).borrow_mut();
        let mut last_tx = LAST_OMNI_TRANSFORM_ACTIVE.borrow(cs).borrow_mut();

        let entering_omni =
            current_kind == PageKind::Omnitrix && *last_kind != Some(PageKind::Omnitrix);
        let exiting_transform =
            (*last_tx) && current_kind == PageKind::Omnitrix && !current_transform_active;

        // update trackers for next frame
        *last_kind = Some(current_kind);
        *last_tx = current_transform_active;

        entering_omni || exiting_transform
    });

    if should_clear_no_fb {
        let _ = if let Some(co) =
            (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
        {
            co.fill_rect_solid_no_fb(0, 0, RESOLUTION as u16, RESOLUTION as u16, Rgb565::BLACK)
                .ok();
        } else {
            disp.clear(Rgb565::BLACK).ok();
        };
    }

    if let Some(dialog) = state.dialog {
        match dialog {
            Dialog::VolumeAdjust => draw_text(
                disp,
                "Adjust Volume (TEMP)",
                Rgb565::WHITE,
                Some(Rgb565::RED),
                CENTER,
                CENTER,
                true,
                true,
            ),
            Dialog::BrightnessAdjust => draw_text(
                disp,
                "Adjust Brightness (TEMP)",
                Rgb565::WHITE,
                Some(Rgb565::MAGENTA),
                CENTER,
                CENTER,
                true,
                true,
            ),
            Dialog::ResetSelector => draw_text(
                disp,
                "Reset? (TEMP)",
                Rgb565::WHITE,
                Some(Rgb565::YELLOW),
                CENTER,
                CENTER,
                true,
                true,
            ),
            Dialog::HomePage => draw_text(
                disp,
                "Home Page (TEMP)",
                Rgb565::GREEN,
                Some(Rgb565::BLACK),
                CENTER,
                CENTER,
                true,
                true,
            ),
            Dialog::StartPage => draw_text(
                disp,
                "Start Page (TEMP)",
                Rgb565::BLUE,
                Some(Rgb565::BLACK),
                CENTER,
                CENTER,
                true,
                true,
            ),
            Dialog::AboutPage => draw_text(
                disp,
                "About Page (TEMP)",
                Rgb565::CYAN,
                Some(Rgb565::BLACK),
                CENTER,
                CENTER,
                true,
                true,
            ),
            Dialog::TransformPage => {
                // show transform overlay, next frame (when dismissed) will clear due to logic above, maybe play an animation if I figure how to do that later
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
                    if let Some((buf, w, h)) = get_cached_asset(AssetId::Logo) {
                        draw_image_bytes(disp, buf, w, h, false, false);
                    } else if precache_asset(AssetId::Logo) {
                        if let Some((buf, w, h)) = get_cached_asset(AssetId::Logo) {
                            draw_image_bytes(disp, buf, w, h, false, false);
                        }
                    }
                }
                MainMenuState::WatchApp => {
                    draw_text(
                        disp,
                        "Watch App (WIP)",
                        Rgb565::WHITE,
                        Some(Rgb565::BLACK),
                        CENTER,
                        CENTER,
                        true,
                        true,
                    );
                }
                MainMenuState::SettingsApp => {
                    draw_text(
                        disp,
                        "Settings (WIP)",
                        Rgb565::WHITE,
                        Some(Rgb565::BLUE),
                        CENTER,
                        CENTER,
                        true,
                        true,
                    );
                }
                MainMenuState::InfoApp => {
                    draw_text(
                        disp,
                        "Easter Egg",
                        Rgb565::WHITE,
                        Some(Rgb565::BLACK),
                        CENTER,
                        CENTER,
                        true,
                        true,
                    );
                }
            }
        }

        Page::Settings(settings_state) => {
            let (msg, fg, bg) = match settings_state {
                SettingsMenuState::Volume => {
                    ("Settings: Volume", Rgb565::YELLOW, Some(Rgb565::BLUE))
                }
                SettingsMenuState::Brightness => {
                    ("Settings: Brightness", Rgb565::YELLOW, Some(Rgb565::BLUE))
                }
                SettingsMenuState::Reset => ("Settings: Reset", Rgb565::YELLOW, Some(Rgb565::BLUE)),
            };
            draw_text(disp, msg, fg, bg, CENTER, CENTER, true, true);
        }

        Page::Watch(watch_state) => {
            let (msg, fg, bg) = match watch_state {
                WatchAppState::Analog => ("Watch: Analog", Rgb565::GREEN, Some(Rgb565::BLACK)),
                WatchAppState::Digital => ("Watch: Digital", Rgb565::CYAN, Some(Rgb565::BLACK)),
            };
            draw_text(disp, msg, fg, bg, CENTER, CENTER, true, true);
        }

        // one layer below main menu home is Omnitrix page
        Page::Omnitrix(omnitrix_state) => {
            // Note that we do not clear here, but before entering a clear happens, it is handled above for efficiency
            let aid = asset_id_for_state(omnitrix_state);
            if let Some((bytes, w, h)) = get_cached_asset(aid) {
                draw_image_bytes(disp, bytes, w, h, false, false);
                // esp_println::println!("Omnitrix: drew cached image");
            } else if precache_asset(aid) {
                if let Some((bytes, w, h)) = get_cached_asset(aid) {
                    draw_image_bytes(disp, bytes, w, h, false, false);
                }
            }
        }

        Page::Info => {
            // Draw info page image; fallback to text if not cached
            if let Some((bytes, w, h)) = get_cached_asset(AssetId::InfoPage) {
                draw_image_bytes(disp, bytes, w, h, false, false);
            } else if precache_asset(AssetId::InfoPage) {
                if let Some((bytes, w, h)) = get_cached_asset(AssetId::InfoPage) {
                    draw_image_bytes(disp, bytes, w, h, false, false);
                }
            } else {
                disp.clear(Rgb565::WHITE).ok();
                draw_text(
                    disp,
                    "Info Screen",
                    Rgb565::CYAN,
                    None,
                    CENTER,
                    CENTER,
                    false,
                    true,
                );
            }
        }
    }
}
