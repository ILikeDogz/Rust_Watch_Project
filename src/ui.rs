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
    image::{Image, ImageRawBE},
    mono_font::{ascii::FONT_10X20, MonoFont, MonoTextStyleBuilder},
    pixelcolor::Rgb565,
    prelude::{OriginDimensions, Point, Primitive, RgbColor, Size},
    primitives::{Line, PrimitiveStyle, Rectangle},
    text::{Alignment, Text},
    Drawable,
};
use esp_hal::timer::systimer::{SystemTimer, Unit};
use libm::{atan2f, cosf, sinf};

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
    SettingsImage,
    WatchIcon,
}

#[derive(Copy, Clone)]
struct AssetSlot {
    data: Option<&'static [u8]>,
    w: u32,
    h: u32,
}

// Number of asset slots
const ASSET_MAX: usize = 14;

macro_rules! res {
    () => {
        "308x374"
    };
} // just a convenience macro for asset paths, a lot have this resolution

// Custom colors
#[allow(dead_code)]
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
    include_bytes!(concat!("assets/debug_image3_466x466_rgb565_be.raw.zlib"));
static SETTINGS_IMAGE: &[u8] = include_bytes!("assets/settings_image_400x344_rgb565_be.raw.zlib");
static WATCH_ICON_IMAGE: &[u8] = include_bytes!("assets/watch_icon_316x316_rgb565_be.raw.zlib");
static WATCH_BG_IMAGE: &[u8] = include_bytes!("assets/watch_background_466x466_rgb565_be.raw.zlib");

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
    EasterEgg,
    Watch,
}
static LAST_PAGE_KIND: Mutex<RefCell<Option<PageKind>>> = Mutex::new(RefCell::new(None));

// Omnitrix transform active tracker
static LAST_OMNI_TRANSFORM_ACTIVE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

// Navigation history management
static NAV_HISTORY: Mutex<RefCell<Vec<Page>>> = Mutex::new(RefCell::new(Vec::new()));
static LAST_WATCH_STATE: Mutex<RefCell<Option<WatchAppState>>> = Mutex::new(RefCell::new(None));
static CLOCK_EDIT: Mutex<RefCell<Option<ClockEditState>>> = Mutex::new(RefCell::new(None));
static LAST_WATCH_EDIT_ACTIVE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
static HAND_CACHE: Mutex<RefCell<HandCache>> = Mutex::new(RefCell::new(HandCache::new()));
static WATCH_BG: Mutex<RefCell<Option<alloc::vec::Vec<u8>>>> = Mutex::new(RefCell::new(None));
static WATCH_FACE_DIRTY: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
static LAST_TRANSFORM_ACTIVE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
static BRIGHTNESS_PCT: Mutex<RefCell<u8>> = Mutex::new(RefCell::new(100));
static BRIGHTNESS_EDIT: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
static BRIGHTNESS_LAST: Mutex<RefCell<Option<u8>>> = Mutex::new(RefCell::new(None));
static LAST_SETTINGS_STATE: Mutex<RefCell<Option<SettingsMenuState>>> =
    Mutex::new(RefCell::new(None));
static BRIGHTNESS_DIRTY: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));

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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
struct ClockEditState {
    digits: [u8; 4], // HHMM digits
    idx: u8,         // active digit 0-3
}

#[derive(Copy, Clone, Default)]
struct HandCache {
    sec: Option<Point>,
    min: Option<Point>,
    hour: Option<Point>,
}

impl HandCache {
    const fn new() -> Self {
        Self {
            sec: None,
            min: None,
            hour: None,
        }
    }
}

// Different pages in the UI
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Page {
    Main(MainMenuState),
    Watch(WatchAppState),
    Settings(SettingsMenuState),
    Omnitrix(OmnitrixState),
    EasterEgg,
}

// Dialogs that can overlay on top of pages
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    TransformPage,
}

// States for Main Menu
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum MainMenuState {
    Home,        // just show home
    WatchApp,    // enter watch app (analog/digital)
    SettingsApp, // enter Settings
}

// States for Watch App
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum WatchAppState {
    Analog,
    Digital,
}

// Simple software clock: base seconds and ticks when set.
static CLOCK_BASE_SECS: Mutex<RefCell<u64>> = Mutex::new(RefCell::new(0));
static CLOCK_BASE_TICKS: Mutex<RefCell<u64>> = Mutex::new(RefCell::new(0));

pub fn set_clock_seconds(seconds: u32) {
    // Set the software clock to the specified seconds since epoch
    let now = SystemTimer::unit_value(Unit::Unit0);
    critical_section::with(|cs| {
        *CLOCK_BASE_SECS.borrow(cs).borrow_mut() = seconds as u64;
        *CLOCK_BASE_TICKS.borrow(cs).borrow_mut() = now;
        *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
        *WATCH_FACE_DIRTY.borrow(cs).borrow_mut() = true;
    });
}

pub fn watch_edit_active() -> bool {
    // Check if clock edit mode is active
    critical_section::with(|cs| CLOCK_EDIT.borrow(cs).borrow().is_some())
}

pub fn watch_edit_start() {
    // Initialize edit state with current time
    let now = clock_now_seconds();
    let total_mins = now / 60;
    let h = ((total_mins / 60) % 24) as u8;
    let m = (total_mins % 60) as u8;
    let digits = [h / 10, h % 10, m / 10, m % 10];

    // Set edit state
    critical_section::with(|cs| {
        *CLOCK_EDIT.borrow(cs).borrow_mut() = Some(ClockEditState { digits, idx: 0 });
    });
}

pub fn watch_edit_cancel() {
    // Clear edit state without committing changes
    critical_section::with(|cs| {
        *CLOCK_EDIT.borrow(cs).borrow_mut() = None;
    });
}

pub fn watch_edit_advance() {
    // Move to next digit or commit changes if on last digit
    critical_section::with(|cs| {
        let mut guard = CLOCK_EDIT.borrow(cs).borrow_mut();
        if let Some(mut ed) = *guard {
            if ed.idx < 3 {
                ed.idx += 1;
                *guard = Some(ed);
            } else {
                // Commit
                let hours = (ed.digits[0] as u32) * 10 + (ed.digits[1] as u32);
                let mins = (ed.digits[2] as u32) * 10 + (ed.digits[3] as u32);
                let secs = (hours * 60 + mins) * 60;
                set_clock_seconds(secs);
                *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
                *WATCH_FACE_DIRTY.borrow(cs).borrow_mut() = true;
                *guard = None;
            }
        }
    });
}

pub fn watch_edit_adjust(delta: i32) {
    // Adjust the active digit by delta (+1 or -1)
    if delta == 0 {
        return;
    }
    critical_section::with(|cs| {
        let mut guard = CLOCK_EDIT.borrow(cs).borrow_mut();
        // Adjust active digit
        if let Some(mut ed) = *guard {
            let idx = ed.idx as usize;
            let mut digit = ed.digits[idx] as i32;
            // Determine min/max for digit
            let (min_d, max_d) = match idx {
                0 => (0, 2),
                1 => {
                    if ed.digits[0] == 2 {
                        (0, 3)
                    } else {
                        (0, 9)
                    }
                }
                2 => (0, 5),
                _ => (0, 9),
            };
            // Adjust digit
            digit += delta;
            // Wrap around
            if digit > max_d {
                digit = min_d;
            }
            if digit < min_d {
                digit = max_d;
            }

            // Update digit
            ed.digits[idx] = digit as u8;
            *guard = Some(ed);
        }
    });
}

pub fn brightness_pct() -> u8 {
    critical_section::with(|cs| *BRIGHTNESS_PCT.borrow(cs).borrow())
}

pub fn brightness_set_pct(new_pct: i32) -> u8 {
    let clamped = new_pct.clamp(0, 100) as u8;
    critical_section::with(|cs| {
        *BRIGHTNESS_PCT.borrow(cs).borrow_mut() = clamped;
        *BRIGHTNESS_DIRTY.borrow(cs).borrow_mut() = true;
    });
    clamped
}

// Adjust brightness by delta, return new percentage
pub fn brightness_adjust(delta: i32) -> u8 {
    if delta == 0 {
        return brightness_pct();
    }
    critical_section::with(|cs| {
        let mut pct = *BRIGHTNESS_PCT.borrow(cs).borrow();
        let mut v = pct as i32 + delta;
        if v < 0 {
            v = 0;
        } else if v > 100 {
            v = 100;
        }
        pct = v as u8;
        // Mark dirty if changed
        if pct != *BRIGHTNESS_PCT.borrow(cs).borrow() {
            *BRIGHTNESS_PCT.borrow(cs).borrow_mut() = pct;
            *BRIGHTNESS_DIRTY.borrow(cs).borrow_mut() = true;
        }
        pct
    })
}

// Check if brightness edit mode is active
pub fn brightness_edit_active() -> bool {
    critical_section::with(|cs| *BRIGHTNESS_EDIT.borrow(cs).borrow())
}

// Set brightness edit mode active/inactive
pub fn brightness_edit_set(active: bool) {
    critical_section::with(|cs| *BRIGHTNESS_EDIT.borrow(cs).borrow_mut() = active);
}

// Take and clear the brightness dirty flag
pub fn brightness_take_dirty() -> bool {
    critical_section::with(|cs| {
        let mut d = BRIGHTNESS_DIRTY.borrow(cs).borrow_mut();
        let was = *d;
        *d = false;
        was
    })
}

// Get the current clock time in seconds since epoch (for saving before deep sleep)
pub fn get_clock_seconds() -> u64 {
    clock_now_seconds()
}

// Clear all cached assets and state (call after waking from deep sleep)
pub fn clear_all_caches() {
    critical_section::with(|cs| {
        // Clear asset cache
        let mut assets = ASSETS.borrow(cs).borrow_mut();
        for slot in assets.iter_mut() {
            slot.data = None;
            slot.w = 0;
            slot.h = 0;
        }

        // Clear page tracking
        *LAST_PAGE_KIND.borrow(cs).borrow_mut() = None;
        *LAST_OMNI_TRANSFORM_ACTIVE.borrow(cs).borrow_mut() = false;
        *NAV_HISTORY.borrow(cs).borrow_mut() = Vec::new();
        *LAST_WATCH_STATE.borrow(cs).borrow_mut() = None;
        *CLOCK_EDIT.borrow(cs).borrow_mut() = None;
        *LAST_WATCH_EDIT_ACTIVE.borrow(cs).borrow_mut() = false;
        *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
        *WATCH_BG.borrow(cs).borrow_mut() = None;
        *WATCH_FACE_DIRTY.borrow(cs).borrow_mut() = false;
        *LAST_TRANSFORM_ACTIVE.borrow(cs).borrow_mut() = false;
        *BRIGHTNESS_LAST.borrow(cs).borrow_mut() = None;
        *LAST_SETTINGS_STATE.borrow(cs).borrow_mut() = None;
        *BRIGHTNESS_DIRTY.borrow(cs).borrow_mut() = false;
    });
}

fn clock_now_seconds() -> u64 {
    // Get current software clock time in seconds since epoch
    critical_section::with(|cs| {
        let base_secs = *CLOCK_BASE_SECS.borrow(cs).borrow();
        let base_ticks = *CLOCK_BASE_TICKS.borrow(cs).borrow();
        let now = SystemTimer::unit_value(Unit::Unit0);
        let tps = SystemTimer::ticks_per_second();
        let elapsed = now.saturating_sub(base_ticks) / tps;
        base_secs.saturating_add(elapsed)
    })
}

pub fn clock_now_seconds_u32() -> u32 {
    clock_now_seconds() as u32
}

fn clock_now_seconds_f32() -> f32 {
    // Get current software clock time in seconds since epoch as f32
    critical_section::with(|cs| {
        let base_secs = *CLOCK_BASE_SECS.borrow(cs).borrow();
        let base_ticks = *CLOCK_BASE_TICKS.borrow(cs).borrow();
        let now = SystemTimer::unit_value(Unit::Unit0);
        let tps = SystemTimer::ticks_per_second() as u64;
        let elapsed_ticks = now.saturating_sub(base_ticks);
        let whole = elapsed_ticks / tps;
        let frac = (elapsed_ticks % tps) as f32 / tps as f32;
        (base_secs + whole) as f32 + frac
    })
}

/// Return hours, minutes, seconds as f32 with good precision by working modulo 12h.
fn clock_now_hms_f32() -> (f32, f32, f32) {
    critical_section::with(|cs| {
        let base_secs = *CLOCK_BASE_SECS.borrow(cs).borrow();
        let base_ticks = *CLOCK_BASE_TICKS.borrow(cs).borrow();
        let now = SystemTimer::unit_value(Unit::Unit0);
        let tps = SystemTimer::ticks_per_second() as u64;
        let elapsed_ticks = now.saturating_sub(base_ticks);
        let whole = elapsed_ticks / tps;
        let frac = (elapsed_ticks % tps) as f32 / tps as f32;
        let total = base_secs + whole;
        let s = (total % 60) as f32 + frac;
        let m_total = total / 60;
        let m = (m_total % 60) as f32 + s / 60.0;
        let h_total = m_total / 60;
        let h = (h_total % 12) as f32 + m / 60.0;
        (h, m, s)
    })
}

// States for Settings Menu
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum SettingsMenuState {
    BrightnessPrompt,
    BrightnessAdjust,
    EasterEgg,
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
                    MainMenuState::SettingsApp => MainMenuState::Home,
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
                    SettingsMenuState::BrightnessPrompt => SettingsMenuState::EasterEgg,
                    SettingsMenuState::EasterEgg => SettingsMenuState::BrightnessPrompt,
                    SettingsMenuState::BrightnessAdjust => SettingsMenuState::BrightnessAdjust,
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
            Page::EasterEgg => Page::EasterEgg,
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
                    MainMenuState::Home => MainMenuState::SettingsApp,
                    MainMenuState::WatchApp => MainMenuState::Home,
                    MainMenuState::SettingsApp => MainMenuState::WatchApp,
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
                    SettingsMenuState::BrightnessPrompt => SettingsMenuState::EasterEgg,
                    SettingsMenuState::EasterEgg => SettingsMenuState::BrightnessPrompt,
                    SettingsMenuState::BrightnessAdjust => SettingsMenuState::BrightnessAdjust,
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
            Page::EasterEgg => Page::EasterEgg,
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
        // If in Settings adjust view, pop back to prompt (also pop nav once).
        if matches!(
            self.page,
            Page::Settings(SettingsMenuState::BrightnessAdjust)
        ) {
            let _ = nav_pop();
            return Self {
                page: Page::Settings(SettingsMenuState::BrightnessPrompt),
                dialog: None,
            };
        }
        if matches!(self.page, Page::EasterEgg) {
            let _ = nav_pop(); // drop the settings->easter egg push
            return Self {
                page: Page::Settings(SettingsMenuState::EasterEgg),
                dialog: None,
            };
        }

        // Otherwise, try navigation history first.
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
                    MainMenuState::SettingsApp => {
                        Page::Settings(SettingsMenuState::BrightnessPrompt)
                    }
                };
                Self { page, dialog: None }
            }
            Page::Watch(_) => Self {
                page: self.page,
                dialog: None,
            },
            Page::Settings(s) => {
                let page = match s {
                    SettingsMenuState::BrightnessPrompt => {
                        nav_push(Page::Settings(s));
                        Page::Settings(SettingsMenuState::BrightnessAdjust)
                    }
                    SettingsMenuState::EasterEgg => {
                        nav_push(Page::Settings(s));
                        Page::EasterEgg
                    }
                    _ => self.page,
                };
                Self { page, dialog: None }
            }
            Page::Omnitrix(_) => Self {
                page: self.page,
                dialog: None,
            }, // changed
            Page::EasterEgg => Self {
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
    font: Option<&'static MonoFont<'static>>,
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
    let font = font.unwrap_or(&FONT_10X20);
    let mut builder = MonoTextStyleBuilder::new().font(font).text_color(fg);
    if let Some(b) = bg {
        builder = builder.background_color(b);
    }
    let style = builder.build();
    Text::with_alignment(text, Point::new(x_point, y_point), style, Alignment::Center)
        .draw(disp)
        .ok();
}

// Format current clock as HH:MM into the provided 5-byte buffer and return it as &str.
fn format_clock_hm(buf: &mut [u8; 5]) -> &str {
    let total_secs = clock_now_seconds();
    let total_mins = total_secs / 60;
    let h = (total_mins / 60) % 24;
    let m = total_mins % 60;

    buf[0] = b'0' + (h / 10) as u8;
    buf[1] = b'0' + (h % 10) as u8;
    buf[2] = b':';
    buf[3] = b'0' + (m / 10) as u8;
    buf[4] = b'0' + (m % 10) as u8;

    core::str::from_utf8(buf).unwrap_or("??:??")
}

fn rgb565_from_888(r: u8, g: u8, b: u8) -> Rgb565 {
    Rgb565::new((r >> 3) as u8, (g >> 2) as u8, (b >> 3) as u8)
}

fn hand_end(cx: i32, cy: i32, angle_deg: f32, length: i32) -> Point {
    let ang = angle_deg.to_radians();
    let dx = (cosf(ang) * length as f32) as i32;
    let dy = (sinf(ang) * length as f32) as i32;
    Point::new(cx + dx, cy + dy)
}

fn draw_hand_line(
    disp: &mut impl PanelRgb565,
    cx: i32,
    cy: i32,
    end: Point,
    color: Rgb565,
    stroke: u8,
) {
    let style = PrimitiveStyle::with_stroke(color, stroke.into());
    let _ = Line::new(Point::new(cx, cy), end)
        .into_styled(style)
        .draw(disp);
}

fn draw_analog_clock(disp: &mut impl PanelRgb565) {
    let center = (RESOLUTION as i32 / 2, RESOLUTION as i32 / 2);
    let cx = center.0;
    let cy = center.1;

    // Current time in fractional hours, minutes, seconds
    let (h, m, s) = clock_now_hms_f32();

    // Angles: 0 deg at 12 o'clock, increasing clockwise
    let sec_ang = (s / 60.0) * 360.0 - 90.0;
    let min_ang = (m / 60.0) * 360.0 - 90.0;
    let hour_ang = (h / 12.0) * 360.0 - 90.0;

    // Hand lengths
    let radius = RESOLUTION as i32 / 2 - 10;
    let sec_len = radius - 10;
    let min_len = radius - 25;
    let hour_len = radius - 50;

    // Compute new endpoints
    let sec_end = hand_end(cx, cy, sec_ang, sec_len);
    let min_end = hand_end(cx, cy, min_ang, min_len);
    let hour_end = hand_end(cx, cy, hour_ang, hour_len);

    // Fast path: draw into FB only and flush once.
    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        let (bbox, _) = critical_section::with(|cs| {
            let mut cache = HAND_CACHE.borrow(cs).borrow_mut();
            let bg_ref = WATCH_BG.borrow(cs).borrow();
            let bgdata = bg_ref.as_ref();

            // Bounding box of old + new hands with padding
            let mut minx = cx;
            let mut miny = cy;
            let mut maxx = cx;
            let mut maxy = cy;
            let mut add_pt = |p: Point, pad: i32| {
                minx = minx.min(p.x - pad);
                miny = miny.min(p.y - pad);
                maxx = maxx.max(p.x + pad);
                maxy = maxy.max(p.y + pad);
            };

            // Add previous hand endpoints
            let sec_stroke = 4;
            let min_stroke = 4;
            let hour_stroke = 4;
            let sec_pad = (sec_stroke * 2).max(6);
            let min_pad = (min_stroke * 2).max(8);
            let hour_pad = (hour_stroke * 2).max(10);

            // Previous points
            if let Some(p) = cache.sec {
                add_pt(p, sec_pad);
            }
            if let Some(p) = cache.min {
                add_pt(p, min_pad);
            }
            if let Some(p) = cache.hour {
                add_pt(p, hour_pad);
            }

            // New points
            add_pt(sec_end, sec_pad);
            add_pt(min_end, min_pad);
            add_pt(hour_end, hour_pad);

            // Center dot padding
            let dot_pad = 22; // covers enlarged center gradient
            add_pt(Point::new(cx, cy), dot_pad);

            // Clear region to background if available, else black
            if let Some(bgdata) = bgdata {
                let bx0 = minx.clamp(0, (RESOLUTION - 1) as i32) as usize;
                let by0 = miny.clamp(0, (RESOLUTION - 1) as i32) as usize;
                let bx1 = maxx.clamp(0, (RESOLUTION - 1) as i32) as usize;
                let by1 = maxy.clamp(0, (RESOLUTION - 1) as i32) as usize;
                let bw = RESOLUTION as usize;
                let w = bx1 - bx0 + 1;
                let h = by1 - by0 + 1;
                let mut buf = alloc::vec::Vec::with_capacity(w * h * 2);
                for row in by0..=by1 {
                    let off = (row * bw + bx0) * 2;
                    buf.extend_from_slice(&bgdata[off..off + w * 2]);
                }
                let _ = co.write_rect_fb(bx0 as u16, by0 as u16, w as u16, h as u16, &buf);
            } else {
                co.fill_rect_fb(minx, miny, maxx, maxy, Rgb565::BLACK);
            }

            // Draw all hands
            // Hour hand
            co.draw_line_fb(
                cx,
                cy,
                hour_end.x,
                hour_end.y,
                Rgb565::WHITE,
                hour_stroke as u8,
            );
            // Minute hand
            co.draw_line_fb(
                cx,
                cy,
                min_end.x,
                min_end.y,
                Rgb565::YELLOW,
                min_stroke as u8,
            );
            // Second hand
            co.draw_line_fb(cx, cy, sec_end.x, sec_end.y, Rgb565::CYAN, sec_stroke as u8);
            // Center dot as solid circle
            let r_outer: i32 = 8;
            let r_outer2: i32 = r_outer * r_outer;
            let c_solid = rgb565_from_888(0x52, 0xC6, 0x6B); // #52C66B
            let x0 = cx - r_outer;
            let y0 = cy - r_outer;
            let x1 = cx + r_outer;
            let y1 = cy + r_outer;
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    let dx = xx - cx;
                    let dy = yy - cy;
                    let d2 = dx * dx + dy * dy;
                    if d2 > r_outer2 {
                        continue;
                    }
                    co.fill_rect_fb(xx, yy, xx, yy, c_solid);
                }
            }

            // Update cache
            cache.sec = Some(sec_end);
            cache.min = Some(min_end);
            cache.hour = Some(hour_end);
            (
                (
                    // Return clamped bbox
                    minx.clamp(0, (RESOLUTION - 1) as i32),
                    miny.clamp(0, (RESOLUTION - 1) as i32),
                    maxx.clamp(0, (RESOLUTION - 1) as i32),
                    maxy.clamp(0, (RESOLUTION - 1) as i32),
                ),
                (),
            )
        });

        // Flush the affected region
        let (minx, miny, maxx, maxy) = bbox;
        let _ = co.flush_rect_even(minx as u16, miny as u16, maxx as u16, maxy as u16);
        return;
    }

    // Fallback: use embedded-graphics path (may flicker more).
    draw_hand_line(disp, cx, cy, sec_end, Rgb565::RED, 2);
    draw_hand_line(disp, cx, cy, min_end, Rgb565::GREEN, 3);
    draw_hand_line(disp, cx, cy, hour_end, Rgb565::BLUE, 4);
}

// Draw an annular arc directly to the panel (no framebuffer update, faster, even-aligned writes).
fn fill_ring_arc_no_fb(
    drv: &mut crate::display::DisplayType<'static>,
    cx: i32,
    cy: i32,
    r_outer: i32,
    r_inner: i32,
    ang0_deg: f32,
    ang1_deg: f32,
    color: Rgb565,
) -> Option<(i32, i32, i32, i32)> {
    // Normalize angles so ang1 >= ang0 in [0, 360+)
    let mut ang0 = ang0_deg;
    let mut ang1 = ang1_deg;
    while ang0 < 0.0 {
        ang0 += 360.0;
        ang1 += 360.0;
    }
    while ang1 < ang0 {
        ang1 += 360.0;
    }
    if ang1 <= ang0 {
        ang1 = ang0 + 360.0;
    }

    // For small arcs, compute a tighter bounding box based on the arc endpoints
    // This dramatically speeds up incremental updates
    let arc_span = ang1 - ang0;
    let (minx, miny, maxx, maxy) = if arc_span < 350.0 {
        // Compute bbox from arc endpoints for BOTH inner and outer radii
        let a0_rad = ang0.to_radians();
        let a1_rad = ang1.to_radians();

        let cos_a0 = cosf(a0_rad);
        let sin_a0 = sinf(a0_rad);
        let cos_a1 = cosf(a1_rad);
        let sin_a1 = sinf(a1_rad);

        // Start with all 4 arc endpoints (inner/outer at start/end angles)
        let outer_x0 = cos_a0 * r_outer as f32;
        let outer_y0 = sin_a0 * r_outer as f32;
        let outer_x1 = cos_a1 * r_outer as f32;
        let outer_y1 = sin_a1 * r_outer as f32;
        let inner_x0 = cos_a0 * r_inner as f32;
        let inner_y0 = sin_a0 * r_inner as f32;
        let inner_x1 = cos_a1 * r_inner as f32;
        let inner_y1 = sin_a1 * r_inner as f32;

        let mut x_min = outer_x0.min(outer_x1).min(inner_x0).min(inner_x1);
        let mut x_max = outer_x0.max(outer_x1).max(inner_x0).max(inner_x1);
        let mut y_min = outer_y0.min(outer_y1).min(inner_y0).min(inner_y1);
        let mut y_max = outer_y0.max(outer_y1).max(inner_y0).max(inner_y1);

        // Check if arc crosses cardinal directions (0°, 90°, 180°, 270°)
        // and extend bbox accordingly using OUTER radius
        let check_angle = |target: f32, ang0: f32, ang1: f32| -> bool {
            let t = if target < ang0 {
                target + 360.0
            } else {
                target
            };
            t >= ang0 && t <= ang1
        };

        if check_angle(0.0, ang0, ang1) {
            x_max = r_outer as f32;
        } // right
        if check_angle(90.0, ang0, ang1) {
            y_max = r_outer as f32;
        } // bottom
        if check_angle(180.0, ang0, ang1) {
            x_min = -(r_outer as f32);
        } // left
        if check_angle(270.0, ang0, ang1) {
            y_min = -(r_outer as f32);
        } // top

        // Convert to screen coords with small padding for rounding errors
        let pad = 2;
        let minx = ((cx + x_min as i32 - pad).max(0)) & !1;
        let maxx = ((cx + x_max as i32 + pad).min((RESOLUTION - 1) as i32)) | 1;
        let miny = ((cy + y_min as i32 - pad).max(0)) & !1;
        let maxy = ((cy + y_max as i32 + pad).min((RESOLUTION - 1) as i32)) | 1;
        (minx, miny, maxx, maxy)
    } else {
        // Full ring - use full bbox
        let minx = ((cx - r_outer).max(0)) & !1;
        let maxx = ((cx + r_outer).min((RESOLUTION - 1) as i32)) | 1;
        let miny = ((cy - r_outer).max(0)) & !1;
        let maxy = ((cy + r_outer).min((RESOLUTION - 1) as i32)) | 1;
        (minx, miny, maxx, maxy)
    };

    let r2_outer = r_outer * r_outer;
    let r2_inner = r_inner * r_inner;

    let mut bb: Option<(i32, i32, i32, i32)> = None;

    // Scan rows in 2-pixel bands to satisfy even-write requirement
    for y0 in (miny..=maxy).step_by(2) {
        let y_center = y0 + 1;
        let dy = y_center - cy;
        // Quick reject if outside outer radius
        if dy * dy > r2_outer {
            continue;
        }
        let mut run_start: Option<i32> = None;
        let mut run_end: i32 = 0;
        for x0 in (minx..=maxx).step_by(2) {
            let x_center = x0 + 1;
            let dx = x_center - cx;
            let d2 = dx * dx + dy * dy;
            let inside_radial = d2 <= r2_outer && d2 >= r2_inner;
            let inside_ang = if inside_radial {
                let mut ang = atan2f(dy as f32, dx as f32).to_degrees();
                if ang < 0.0 {
                    ang += 360.0;
                }
                if ang < ang0 {
                    ang += 360.0;
                }
                ang >= ang0 && ang <= ang1
            } else {
                false
            };

            if inside_ang {
                if run_start.is_none() {
                    run_start = Some(x0);
                }
                run_end = x0;
            } else if let Some(rs) = run_start {
                let width = (run_end - rs + 2) as u16;
                let _ = drv.fill_rect_solid_no_fb(rs as u16, y0 as u16, width, 2, color);
                bb = Some(match bb {
                    None => (rs, y0, rs + width as i32 - 1, y0 + 1),
                    Some((bx0, by0, bx1, by1)) => (
                        bx0.min(rs),
                        by0.min(y0),
                        bx1.max(rs + width as i32 - 1),
                        by1.max(y0 + 1),
                    ),
                });
                run_start = None;
            }
        }
        if let Some(rs) = run_start {
            let width = (run_end - rs + 2) as u16;
            let _ = drv.fill_rect_solid_no_fb(rs as u16, y0 as u16, width, 2, color);
            bb = Some(match bb {
                None => (rs, y0, rs + width as i32 - 1, y0 + 1),
                Some((bx0, by0, bx1, by1)) => (
                    bx0.min(rs),
                    by0.min(y0),
                    bx1.max(rs + width as i32 - 1),
                    by1.max(y0 + 1),
                ),
            });
        }
    }
    bb
}

fn draw_ring_segment(
    disp: &mut impl PanelRgb565,
    cx: i32,
    cy: i32,
    radius: i32,
    thickness: i32,
    start_deg: f32,
    end_deg: f32,
    color: Rgb565,
) {
    // Draw radial lines at intervals to form ring segment
    let step = 3.0_f32;
    let r_inner = radius.saturating_sub(thickness.max(1) - 1);

    // Fast path: draw into FB only and flush once.
    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        let mut minx = i32::MAX;
        let mut miny = i32::MAX;
        let mut maxx = i32::MIN;
        let mut maxy = i32::MIN;

        // Draw line and update bbox
        let mut draw_line = |x0: i32, y0: i32, x1: i32, y1: i32| {
            if let Some((ax0, ay0, ax1, ay1)) =
                co.draw_line_fb(x0, y0, x1, y1, color, thickness as u8)
            {
                minx = minx.min(ax0 as i32);
                miny = miny.min(ay0 as i32);
                maxx = maxx.max(ax1 as i32);
                maxy = maxy.max(ay1 as i32);
            }
        };

        // Draw all radial lines
        let mut a = start_deg;
        while a <= end_deg + 0.1 {
            let ar = a.to_radians();
            let ox = cx + (cosf(ar) * radius as f32) as i32;
            let oy = cy + (sinf(ar) * radius as f32) as i32;
            let ix = cx + (cosf(ar) * r_inner as f32) as i32;
            let iy = cy + (sinf(ar) * r_inner as f32) as i32;
            draw_line(ox, oy, ix, iy);
            a += step;
        }

        // Flush affected region
        if minx != i32::MAX {
            let _ = co.flush_rect_even(
                minx.clamp(0, (RESOLUTION - 1) as i32) as u16,
                miny.clamp(0, (RESOLUTION - 1) as i32) as u16,
                maxx.clamp(0, (RESOLUTION - 1) as i32) as u16,
                maxy.clamp(0, (RESOLUTION - 1) as i32) as u16,
            );
        }
    } else {
        // Fallback: use embedded-graphics path (may flicker more).
        let mut a = start_deg;
        while a <= end_deg + 0.1 {
            let ar = a.to_radians();
            let ox = cx + (cosf(ar) * radius as f32) as i32;
            let oy = cy + (sinf(ar) * radius as f32) as i32;
            let ix = cx + (cosf(ar) * r_inner as f32) as i32;
            let iy = cy + (sinf(ar) * r_inner as f32) as i32;
            let _ = Line::new(Point::new(ox, oy), Point::new(ix, iy))
                .into_styled(PrimitiveStyle::with_stroke(color, thickness.max(1) as u32))
                .draw(disp);
            a += step;
        }
    }
}

fn draw_brightness_ui(disp: &mut impl PanelRgb565) {
    let pct = brightness_pct();
    let radius = (RESOLUTION as i32 / 2) + 10;
    let thickness_fg = 20;
    let thickness_bg = thickness_fg + 12;
    let radius_fg_outer = radius;
    let radius_fg_inner = radius - thickness_fg;
    let radius_bg_outer = radius + 2;
    let radius_bg_inner = (radius - thickness_bg - 2).max(0);
    let start = -90.0_f32;
    let end_full = start + 360.0;
    let end_pct = start + (pct as f32) * 3.6;
    let bg_ring = Rgb565::BLACK;
    let fg_ring = rgb565_from_888(0x9F, 0xFF, 0x4A);

    let pad = radius_bg_outer + 4;
    let x0 = (CENTER - pad).clamp(0, (RESOLUTION - 1) as i32);
    let x1 = (CENTER + pad).clamp(0, (RESOLUTION - 1) as i32);
    let y0 = (CENTER - pad).clamp(0, (RESOLUTION - 1) as i32);
    let y1 = (CENTER + pad).clamp(0, (RESOLUTION - 1) as i32);
    // Tight text box so we don't wipe nearby graphics.
    let text_box = (CENTER - 70, CENTER - 20, CENTER + 70, CENTER + 20);

    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        let prev_pct_opt = critical_section::with(|cs| *BRIGHTNESS_LAST.borrow(cs).borrow());
        let do_full = prev_pct_opt.is_none();
        let prev_pct = prev_pct_opt.unwrap_or(pct);

        let prev_ang = start + (prev_pct as f32) * 3.6;
        let new_ang = start + (pct as f32) * 3.6;

        if do_full {
            // Full redraw: background then foreground
            let _ = fill_ring_arc_no_fb(
                co,
                CENTER,
                CENTER,
                radius_bg_outer,
                radius_bg_inner,
                start - 5.0,
                end_full + 5.0,
                bg_ring,
            );
            if pct > 0 {
                let fg_end = if pct == 100 { end_full + 5.0 } else { new_ang };
                let _ = fill_ring_arc_no_fb(
                    co,
                    CENTER,
                    CENTER,
                    radius_fg_outer,
                    radius_fg_inner,
                    start - 5.0,
                    fg_end,
                    fg_ring,
                );
            }
        } else if pct != prev_pct {
            // Incremental update - use SAME radii for both clear and paint
            // Use the bg radii for everything to ensure consistent ring shape
            let delta = (pct as i32) - (prev_pct as i32);

            if delta > 0 {
                // GROWING: paint the new segment with fg radii
                let fg_start = (prev_ang - 2.0).max(start - 5.0);
                let fg_end = if pct == 100 {
                    end_full + 5.0
                } else {
                    new_ang + 2.0
                };
                let _ = fill_ring_arc_no_fb(
                    co,
                    CENTER,
                    CENTER,
                    radius_fg_outer,
                    radius_fg_inner,
                    fg_start,
                    fg_end,
                    fg_ring,
                );
            } else {
                // SHRINKING:
                // 1. First clear the entire area from new_ang to prev_ang using bg radii
                let clear_start = if pct == 0 { start - 5.0 } else { new_ang - 2.0 };
                let clear_end = prev_ang + 5.0;
                let _ = fill_ring_arc_no_fb(
                    co,
                    CENTER,
                    CENTER,
                    radius_bg_outer,
                    radius_bg_inner,
                    clear_start,
                    clear_end,
                    bg_ring,
                );
                // 2. Repaint the tip AND the outer/inner edges to restore clean boundary
                if pct > 0 {
                    // Repaint a small segment of the foreground to clean up the edge
                    let _ = fill_ring_arc_no_fb(
                        co,
                        CENTER,
                        CENTER,
                        radius_fg_outer,
                        radius_fg_inner,
                        new_ang - 5.0,
                        new_ang + 2.0,
                        fg_ring,
                    );
                }
            }
        }

        // Update text
        let (tx0, ty0, tx1, ty1) = text_box;
        co.fill_rect_fb(tx0, ty0, tx1, ty1, Rgb565::BLACK);
        let pct_buf = alloc::format!("{}%", pct);
        draw_text(
            co,
            &pct_buf,
            fg_ring,
            None,
            CENTER,
            CENTER,
            false,
            true,
            Some(&FONT_10X20),
        );

        critical_section::with(|cs| {
            *BRIGHTNESS_LAST.borrow(cs).borrow_mut() = Some(pct);
        });

        // Flush only text box
        let fx0 = (tx0.clamp(0, (RESOLUTION - 1) as i32)) & !1;
        let fy0 = (ty0.clamp(0, (RESOLUTION - 1) as i32)) & !1;
        let fx1 = (tx1.clamp(0, (RESOLUTION - 1) as i32) | 1).min((RESOLUTION - 1) as i32);
        let fy1 = (ty1.clamp(0, (RESOLUTION - 1) as i32) | 1).min((RESOLUTION - 1) as i32);
        let _ = co.flush_rect_even(fx0 as u16, fy0 as u16, fx1 as u16, fy1 as u16);
    } else {
        // Fallback: small clear and redraw (non-panel path).
        let _ = Rectangle::new(
            Point::new(x0, y0),
            Size::new((x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(disp);
        draw_ring_segment(
            disp,
            CENTER,
            CENTER,
            radius,
            thickness_bg,
            start,
            end_full,
            bg_ring,
        );
        draw_ring_segment(
            disp,
            CENTER,
            CENTER,
            radius,
            thickness_bg,
            start,
            end_pct,
            fg_ring,
        );
        draw_ring_segment(
            disp,
            CENTER,
            CENTER,
            radius,
            thickness_fg,
            start,
            end_pct,
            fg_ring,
        );
        // Text: redraw center text in fallback mode
        let pct_buf = alloc::format!("{}%", pct);
        draw_text(
            disp,
            &pct_buf,
            fg_ring,
            None,
            CENTER,
            CENTER - 8,
            false,
            true,
            Some(&FONT_10X20),
        );
    }
}

fn draw_transform_overlay(disp: &mut impl PanelRgb565) {
    // DNA-like helix animation with depth sorting for proper 3D illusion
    let t = clock_now_seconds_f32() * 1.6; // slower rotation for better 3D illusion
    let amp_max = (RESOLUTION as f32) * 0.26;
    let step = 16; // slightly tighter spacing for smoother curve
    let cx = CENTER;
    let y_start = 12;
    let y_end = RESOLUTION as i32 - 12;

    // Front/back color pairs with more contrast for depth
    let strand_a_front = rgb565_from_888(0xC0, 0xFF, 0x70); // brighter front
    let strand_a_back = rgb565_from_888(0x40, 0x90, 0x10); // darker back
    let strand_b_front = rgb565_from_888(0xA8, 0xFF, 0x50);
    let strand_b_back = rgb565_from_888(0x38, 0x80, 0x08);
    let rung_front = rgb565_from_888(0xB0, 0xFF, 0x60);
    let rung_back = rgb565_from_888(0x50, 0x90, 0x18);

    // Base thickness values - will be modulated by depth
    let strand_thick_base = 6u8;
    let rung_thick = 3u8;

    // Bounding box for the helix drawing (reuse for clear/flush).
    let pad = (amp_max as i32 + 20).min(CENTER);
    let x0 = (cx - pad).clamp(0, (RESOLUTION - 1) as i32);
    let x1 = (cx + pad).clamp(0, (RESOLUTION - 1) as i32);
    let y0 = (y_start - 8).clamp(0, (RESOLUTION - 1) as i32);
    let y1 = (y_end + 8).clamp(0, (RESOLUTION - 1) as i32);

    if let Some(co) = (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
    {
        // Clear only the helix region in the framebuffer each frame.
        co.fill_rect_fb(x0, y0, x1, y1, Rgb565::BLACK);

        // Collect strand segments for depth-sorted drawing
        // (y_pos, depth, is_strand_a, prev_point, curr_point)
        let mut segments: heapless::Vec<(i32, f32, bool, Point, Point), 64> = heapless::Vec::new();

        // Collect rungs with depth info for proper front/back coloring
        // (y_pos, depth, point_a, point_b, is_front)
        let mut rungs: heapless::Vec<(i32, f32, Point, Point, bool), 32> = heapless::Vec::new();

        let mut prev_a: Option<Point> = None;
        let mut prev_b: Option<Point> = None;

        // Generate strand points
        for (i, y) in (y_start..=y_end).step_by(step).enumerate() {
            let phase = t + (i as f32) * 0.32;
            let amp = amp_max * 0.75;

            let off_a = (sinf(phase) * amp) as i32;
            let off_b = -off_a;

            let xa = cx + off_a;
            let xb = cx + off_b;
            let pa = Point::new(xa, y);
            let pb = Point::new(xb, y);

            // Depth value: cosf gives z-depth (-1 = back, +1 = front)
            let depth_a = cosf(phase);
            // let depth_b = -depth_a;

            if let (Some(pa_prev), Some(pb_prev)) = (prev_a, prev_b) {
                let prev_phase = t + ((i - 1) as f32) * 0.32;
                let avg_depth_a = (depth_a + cosf(prev_phase)) / 2.0;
                let avg_depth_b = -avg_depth_a;

                let _ = segments.push((y, avg_depth_a, true, pa_prev, pa));
                let _ = segments.push((y, avg_depth_b, false, pb_prev, pb));
            }

            // Draw rungs at fixed Y intervals
            if i % 3 == 1 {
                // Rung visibility based on rotation: when strands are at edges (|sinf| high),
                // the rung is facing us or away. When |sinf| is low, rung is on the side.
                // Use cosf to determine if rung faces front or back
                let rung_facing_front = cosf(phase).abs() < 0.7; // rung visible when strands near edges
                let rung_depth = if rung_facing_front { 0.1 } else { -0.5 };
                let _ = rungs.push((y, rung_depth, pa, pb, rung_facing_front));
            }

            prev_a = Some(pa);
            prev_b = Some(pb);
        }

        // Sort strands by depth (back-to-front)
        for i in 0..segments.len() {
            for j in 0..segments.len().saturating_sub(1 + i) {
                if segments[j].1 > segments[j + 1].1 {
                    segments.swap(j, j + 1);
                }
            }
        }

        // Sort rungs by depth too
        for i in 0..rungs.len() {
            for j in 0..rungs.len().saturating_sub(1 + i) {
                if rungs[j].1 > rungs[j + 1].1 {
                    rungs.swap(j, j + 1);
                }
            }
        }

        // Interleave drawing: back rungs, back strands, front rungs, front strands
        // Draw back rungs first
        for &(_y, depth, pa, pb, is_front) in rungs.iter() {
            if depth < 0.0 {
                let col = if is_front { rung_front } else { rung_back };
                let _ = co.draw_line_fb(pa.x, pa.y, pb.x, pb.y, col, rung_thick);
            }
        }

        // Draw sorted strand segments (back ones first due to sorting)
        for &(_y, depth, is_a, p_prev, p_curr) in segments.iter() {
            let depth_factor = (depth + 1.0) / 2.0;
            let strand_thick = ((strand_thick_base as f32) * (0.5 + 0.7 * depth_factor)) as u8;
            let strand_thick = strand_thick.max(3).min(9);

            let front_side = depth >= 0.0;

            let (col_main, col_shadow) = if is_a {
                if front_side {
                    (strand_a_front, rgb565_from_888(0x70, 0xB0, 0x30))
                } else {
                    (strand_a_back, rgb565_from_888(0x28, 0x60, 0x08))
                }
            } else {
                if front_side {
                    (strand_b_front, rgb565_from_888(0x60, 0xA0, 0x28))
                } else {
                    (strand_b_back, rgb565_from_888(0x20, 0x50, 0x04))
                }
            };

            let _ = co.draw_line_fb(
                p_prev.x,
                p_prev.y,
                p_curr.x,
                p_curr.y,
                col_shadow,
                strand_thick + 2,
            );
            let _ = co.draw_line_fb(
                p_prev.x,
                p_prev.y,
                p_curr.x,
                p_curr.y,
                col_main,
                strand_thick,
            );
        }

        // Draw front rungs last (on top of strands)
        for &(_y, depth, pa, pb, is_front) in rungs.iter() {
            if depth >= 0.0 {
                let col = if is_front { rung_front } else { rung_back };
                let _ = co.draw_line_fb(pa.x, pa.y, pb.x, pb.y, col, rung_thick);
            }
        }

        // Flush only the helix region to avoid needless panel churn.
        let _ = co.flush_rect_even(x0 as u16, y0 as u16, x1 as u16, y1 as u16);
    } else {
        // Fallback path using embedded-graphics primitives.
        let strand_thick = strand_thick_base; // use base thickness for fallback
        let _ = Rectangle::new(
            Point::new(x0, y0),
            Size::new((x1 - x0 + 1) as u32, (y1 - y0 + 1) as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(disp);
        let mut prev_a: Option<Point> = None;
        let mut prev_b: Option<Point> = None;

        // Draw helix strands
        for (i, y) in (y_start..=y_end).step_by(step).enumerate() {
            let phase = t + (i as f32) * 0.35;
            let amp = amp_max * 0.75;
            let off = (sinf(phase) * amp) as i32;
            let xa = cx + off;
            let xb = cx - off;
            let pa = Point::new(xa, y);
            let pb = Point::new(xb, y);
            let front_side = sinf(phase) >= 0.0;

            // Choose colors based on front/back
            let col_a = if front_side {
                strand_a_front
            } else {
                strand_a_back
            };
            let col_b = if front_side {
                strand_b_back
            } else {
                strand_b_front
            };
            let col_a_sh = rgb565_from_888(
                (col_a.r().saturating_mul(3) / 4) as u8,
                (col_a.g().saturating_mul(3) / 4) as u8,
                (col_a.b().saturating_mul(3) / 4) as u8,
            );
            let col_b_sh = rgb565_from_888(
                (col_b.r().saturating_mul(3) / 4) as u8,
                (col_b.g().saturating_mul(3) / 4) as u8,
                (col_b.b().saturating_mul(3) / 4) as u8,
            );

            // Connect strands smoothly
            if let Some(p) = prev_a {
                let _ = Line::new(p, pa)
                    .into_styled(PrimitiveStyle::with_stroke(col_a_sh, strand_thick.into()))
                    .draw(disp);
                let _ = Line::new(p, pa)
                    .into_styled(PrimitiveStyle::with_stroke(
                        col_a,
                        strand_thick.saturating_sub(2).into(),
                    ))
                    .draw(disp);
            }

            // Connect strands smoothly
            if let Some(p) = prev_b {
                let _ = Line::new(p, pb)
                    .into_styled(PrimitiveStyle::with_stroke(col_b_sh, strand_thick.into()))
                    .draw(disp);
                let _ = Line::new(p, pb)
                    .into_styled(PrimitiveStyle::with_stroke(
                        col_b,
                        strand_thick.saturating_sub(2).into(),
                    ))
                    .draw(disp);
            }

            // Curved rung: bend slightly using a midpoint offset for a faux spin effect.
            let mid_phase = phase + core::f32::consts::FRAC_PI_2;
            let mid_bend = (sinf(mid_phase) * amp * 0.18) as i32;
            let mid_x = cx + mid_bend;
            let mid_y = y + step as i32 / 2;
            let pm = Point::new(mid_x, mid_y);
            let col_rung = if front_side { rung_front } else { rung_back };

            // Draw two segments to form a bent rung
            let _ = Line::new(pa, pm)
                .into_styled(PrimitiveStyle::with_stroke(col_rung, rung_thick.into()))
                .draw(disp);
            let _ = Line::new(pm, pb)
                .into_styled(PrimitiveStyle::with_stroke(col_rung, rung_thick.into()))
                .draw(disp);

            prev_a = Some(pa);
            prev_b = Some(pb);
        }
    }
}

fn draw_clock_edit(disp: &mut impl PanelRgb565, ed: ClockEditState) {
    // Build HH:MM string from digits
    let mut buf = [b'0'; 5];
    buf[0] = b'0' + ed.digits[0];
    buf[1] = b'0' + ed.digits[1];
    buf[2] = b':';
    buf[3] = b'0' + ed.digits[2];
    buf[4] = b'0' + ed.digits[3];
    let msg = core::str::from_utf8(&buf).unwrap_or("00:00");

    let font = &FONT_10X20; // largest built-in mono ASCII font available

    // Draw the time (use larger 10x20 font)
    draw_text(
        disp,
        msg,
        Rgb565::CYAN,
        Some(Rgb565::BLACK),
        CENTER,
        CENTER,
        false,
        true,
        Some(font),
    );

    // Underline the active digit only (skip the colon)
    let char_w = font.character_size.width as i32;
    let char_h = font.character_size.height as i32;
    let chars_total = 5;
    let box_w = char_w * chars_total;
    let start_x = CENTER - box_w / 2;
    let base_y = CENTER + char_h / 2 + 2;
    let idx = ed.idx.min(3) as i32;
    let visual_idx = if idx >= 2 { idx + 1 } else { idx }; // skip colon slot
    let underline_x = start_x + visual_idx * char_w;

    // Draw underline rectangle
    let rect = Rectangle::new(Point::new(underline_x, base_y), Size::new(char_w as u32, 2));
    rect.into_styled(PrimitiveStyle::with_fill(Rgb565::CYAN))
        .draw(disp)
        .ok();
}

fn ensure_watch_background_loaded() -> bool {
    // Decompress watch background into PSRAM if not already done
    critical_section::with(|cs| {
        if WATCH_BG.borrow(cs).borrow().is_some() {
            return true;
        }

        // Decompress now
        if let Ok(decompressed) = decompress_to_vec_zlib_with_limit(
            WATCH_BG_IMAGE,
            (RESOLUTION * RESOLUTION * 2) as usize,
        ) {
            *WATCH_BG.borrow(cs).borrow_mut() = Some(decompressed);
            true
        } else {
            false
        }
    })
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
        AssetId::SettingsImage => (12, 400, 344, SETTINGS_IMAGE),
        AssetId::WatchIcon => (13, 316, 316, WATCH_ICON_IMAGE),
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
        AssetId::SettingsImage,
        AssetId::WatchIcon,
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
        Page::EasterEgg => PageKind::EasterEgg,
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
            Dialog::TransformPage => {
                // On first entry into Transform dialog, hard clear the whole screen.
                let entering = critical_section::with(|cs| {
                    let mut last = LAST_TRANSFORM_ACTIVE.borrow(cs).borrow_mut();
                    let was = *last;
                    *last = true;
                    !was
                });
                if entering {
                    if let Some(co) = (disp as &mut dyn Any)
                        .downcast_mut::<crate::display::DisplayType<'static>>()
                    {
                        let _ = co.fill_rect_solid_no_fb(
                            0,
                            0,
                            RESOLUTION as u16,
                            RESOLUTION as u16,
                            Rgb565::BLACK,
                        );
                        co.fill_rect_fb(
                            0,
                            0,
                            (RESOLUTION - 1) as i32,
                            (RESOLUTION - 1) as i32,
                            Rgb565::BLACK,
                        );
                    } else {
                        let _ = disp.clear(Rgb565::BLACK);
                    }
                }

                draw_transform_overlay(disp);
            }
        }
        return;
    }

    // Reset watch-state tracker if we’re not on the Watch page.
    if !matches!(state.page, Page::Watch(_)) {
        critical_section::with(|cs| {
            *LAST_WATCH_STATE.borrow(cs).borrow_mut() = None;
            *WATCH_BG.borrow(cs).borrow_mut() = None; // free background when leaving watch page
            *LAST_WATCH_EDIT_ACTIVE.borrow(cs).borrow_mut() = false;
        });
    }
    let entering_brightness = critical_section::with(|cs| {
        let mut last = LAST_SETTINGS_STATE.borrow(cs).borrow_mut();
        let was = *last;
        let now = if let Page::Settings(s) = state.page {
            Some(s)
        } else {
            None
        };
        *last = now;
        was != now && matches!(now, Some(SettingsMenuState::BrightnessAdjust))
    });
    if !matches!(state.page, Page::Settings(_)) {
        brightness_edit_set(false);
        critical_section::with(|cs| *BRIGHTNESS_LAST.borrow(cs).borrow_mut() = None);
    } else {
        // Within settings: clear brightness edit when not on brightness adjust page, and reset cache when entering adjust.
        if !matches!(
            state.page,
            Page::Settings(SettingsMenuState::BrightnessAdjust)
        ) {
            brightness_edit_set(false);
        }
        if entering_brightness {
            critical_section::with(|cs| *BRIGHTNESS_LAST.borrow(cs).borrow_mut() = None);
        }
    }
    // Reset transform tracker when dialog is not active.
    critical_section::with(|cs| {
        *LAST_TRANSFORM_ACTIVE.borrow(cs).borrow_mut() = false;
    });

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
                    let _ = disp.clear(Rgb565::BLACK);
                    if let Some((bytes, w, h)) = get_cached_asset(AssetId::WatchIcon) {
                        draw_image_bytes(disp, bytes, w, h, false, false);
                    } else if precache_asset(AssetId::WatchIcon) {
                        if let Some((bytes, w, h)) = get_cached_asset(AssetId::WatchIcon) {
                            draw_image_bytes(disp, bytes, w, h, false, false);
                        }
                    }
                }
                MainMenuState::SettingsApp => {
                    let _ = disp.clear(Rgb565::BLACK);
                    if let Some((bytes, w, h)) = get_cached_asset(AssetId::SettingsImage) {
                        draw_image_bytes(disp, bytes, w, h, false, false);
                    } else if precache_asset(AssetId::SettingsImage) {
                        if let Some((bytes, w, h)) = get_cached_asset(AssetId::SettingsImage) {
                            draw_image_bytes(disp, bytes, w, h, false, false);
                        }
                    }
                }
            }
        }

        Page::Settings(settings_state) => match settings_state {
            SettingsMenuState::BrightnessPrompt => {
                // Clear the screen, then draw a simple white sun icon with label inside.
                let _ = disp.clear(Rgb565::BLACK);
                let cx = CENTER;
                let cy = CENTER;
                let outer_r = 90;
                let ray_len = 42;
                let ray_thick = 6u8;
                let col = Rgb565::WHITE;
                // Circle + rays using embedded-graphics primitives.
                let _ = embedded_graphics::primitives::Circle::new(
                    Point::new(cx - outer_r, cy - outer_r),
                    (outer_r * 2) as u32,
                )
                .into_styled(PrimitiveStyle::with_stroke(col, 4))
                .draw(disp);
                for i in 0..8 {
                    let ang = i as f32 * core::f32::consts::FRAC_PI_4;
                    let dx = (cosf(ang) * (outer_r + 4) as f32) as i32;
                    let dy = (sinf(ang) * (outer_r + 4) as f32) as i32;
                    let tx = cx + dx;
                    let ty = cy + dy;
                    let rx = (cosf(ang) * (outer_r + ray_len) as f32) as i32 + cx;
                    let ry = (sinf(ang) * (outer_r + ray_len) as f32) as i32 + cy;
                    let _ = Line::new(Point::new(tx, ty), Point::new(rx, ry))
                        .into_styled(PrimitiveStyle::with_stroke(col, ray_thick as u32))
                        .draw(disp);
                }

                // two layers of text to fit the sun icon
                draw_text(
                    disp,
                    "Adjust",
                    col,
                    Some(Rgb565::BLACK),
                    CENTER,
                    CENTER - 8,
                    false,
                    false,
                    None,
                );
                // second layer for better readability
                draw_text(
                    disp,
                    "Brightness",
                    col,
                    Some(Rgb565::BLACK),
                    CENTER,
                    CENTER + 8,
                    false,
                    false,
                    None,
                );
            }
            SettingsMenuState::BrightnessAdjust => {
                draw_brightness_ui(disp);
            }
            SettingsMenuState::EasterEgg => {
                draw_text(
                    disp,
                    "Easter Egg",
                    Rgb565::WHITE,
                    Some(Rgb565::BLACK),
                    CENTER,
                    CENTER,
                    true,
                    true,
                    None,
                );
            }
        },

        Page::Watch(watch_state) => {
            // If watch mode changed, repaint face and reset cache.
            let should_clear_watch = critical_section::with(|cs| {
                let mut last = LAST_WATCH_STATE.borrow(cs).borrow_mut();
                let changed = *last != Some(watch_state);
                *last = Some(watch_state);
                changed
            });

            if should_clear_watch {
                // Reload background
                if ensure_watch_background_loaded() {
                    critical_section::with(|cs| {
                        if let Some(bg) = WATCH_BG.borrow(cs).borrow().as_ref() {
                            draw_image_bytes(disp, bg, RESOLUTION, RESOLUTION, false, true);
                        }
                    });
                }
                critical_section::with(|cs| {
                    *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
                });
            }

            // If time was changed, repaint face and reset cache.
            let face_dirty = critical_section::with(|cs| {
                let mut f = WATCH_FACE_DIRTY.borrow(cs).borrow_mut();
                let dirty = *f;
                if dirty {
                    *f = false;
                }
                dirty
            });

            // If dirty, reload background and reset hand cache.
            if face_dirty {
                if ensure_watch_background_loaded() {
                    critical_section::with(|cs| {
                        if let Some(bg) = WATCH_BG.borrow(cs).borrow().as_ref() {
                            draw_image_bytes(disp, bg, RESOLUTION, RESOLUTION, false, true);
                        }
                    });
                }
                critical_section::with(|cs| {
                    *HAND_CACHE.borrow(cs).borrow_mut() = HandCache::new();
                });
            }

            match watch_state {
                WatchAppState::Analog => {
                    draw_analog_clock(disp);
                }
                WatchAppState::Digital => {
                    // Draw either time or edit state
                    let edit = critical_section::with(|cs| *CLOCK_EDIT.borrow(cs).borrow());
                    let should_clear_after_edit = critical_section::with(|cs| {
                        let mut last = LAST_WATCH_EDIT_ACTIVE.borrow(cs).borrow_mut();
                        let was = *last;
                        let now = edit.is_some();
                        *last = now;
                        was && !now
                    });

                    // If we were in edit mode last frame but not now, need to clear to bg
                    if should_clear_after_edit {
                        if ensure_watch_background_loaded() {
                            if let Some(bg) = critical_section::with(|cs| {
                                WATCH_BG.borrow(cs).borrow().as_ref().cloned()
                            }) {
                                draw_image_bytes(disp, &bg, RESOLUTION, RESOLUTION, false, true);
                            }
                        }
                    }

                    // Draw either edit UI or current time
                    if let Some(ed) = edit {
                        draw_clock_edit(disp, ed);
                    } else {
                        let mut buf = [b'0'; 5];
                        let msg = format_clock_hm(&mut buf);
                        draw_text(
                            disp,
                            msg,
                            Rgb565::CYAN,
                            Some(Rgb565::BLACK),
                            CENTER,
                            CENTER,
                            false,
                            true,
                            None,
                        );
                    }
                }
            }
        }

        // one layer below main menu home is Omnitrix page
        Page::Omnitrix(omnitrix_state) => {
            // Note that we do not clear here, but before entering a clear happens, it is handled above for efficiency
            // Clear is necessary as the alien images don't cover the full screen
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

        Page::EasterEgg => {
            // Draw info page image by decompressing on demand (no cache).
            let need = (466 * 466 * 2) as usize;
            if let Ok(buf) = decompress_to_vec_zlib_with_limit(INFO_PAGE_IMAGE, need) {
                if buf.len() == need {
                    draw_image_bytes(disp, &buf, 466, 466, false, false);
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
                        None,
                    );
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
                    None,
                );
            }
        }
    }
}
