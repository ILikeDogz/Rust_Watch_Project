// UI state management and display rendering module.
//
// Split into smaller modules:
// - `state` for navigation enums and helpers
// - `assets` for decompression/caching/blitting images
// - `brightness` and `time` for user-adjustable state
// - `pages::*` for page-specific renderers

extern crate alloc;

use core::any::Any;
use core::cell::RefCell;

use critical_section::Mutex;
use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::Rgb565,
    prelude::{OriginDimensions, RgbColor},
};

pub mod assets;
pub mod brightness;
pub mod draw;
pub mod games;
pub mod pages;
pub mod state;
pub mod time;

pub use assets::{get_cached_asset, precache_all, precache_asset, AssetId};
pub use draw::draw_image_bytes;
pub use brightness::{
    brightness_adjust, brightness_edit_active, brightness_edit_set, brightness_take_dirty,
    get_brightness_pct,
};
pub use games::pong::{
    pong_ball_active, pong_ball_attached_pos, pong_ball_last_pos, pong_ball_pos,
    pong_ball_set_last_pos, pong_ball_set_pos, pong_ball_start, pong_ball_update, pong_game_over,
    pong_paddle_adjust_timed, pong_paddle_angle, pong_paddle_flip_timed, pong_paddle_last_angle,
    pong_paddle_set_last_angle, pong_play_radius, pong_score, pong_reset_on_exit, pong_set_game_over,
    pong_set_text_visible, pong_text_visible, pong_win, PONG_BALL_RADIUS, PONG_PADDLE_ARC_DEG,
    PONG_PADDLE_RADIUS_PAD, PONG_PADDLE_STROKE, PONG_PADDLE_THICKNESS,
};
pub use games::snake::{
    play_snake, snake_active, snake_reset_on_exit, snake_start, snake_turn_button,
    snake_turn_steps, snake_update,
};
pub use state::{
    Dialog, GamesState, MainMenuState, OmnitrixState, Page, SettingsMenuState, UiState,
    WatchAppState,
};
pub use time::{
    clock_now_seconds_u32, format_clock_hm, get_clock_seconds, set_clock_seconds,
    watch_edit_active, watch_edit_adjust, watch_edit_advance, watch_edit_cancel, watch_edit_start,
};

// Display configuration, (0,0) is top-left corner
pub const RESOLUTION: u32 = 466;
pub const CENTER: i32 = (RESOLUTION / 2) as i32;

// Make a lightweight trait bound we’ll use for the factory’s return type.
pub trait PanelRgb565: DrawTarget<Color = Rgb565> + OriginDimensions + Any {}
impl<T> PanelRgb565 for T where T: DrawTarget<Color = Rgb565> + OriginDimensions + Any {}

// Page kind tracker for optimization
static LAST_PAGE_KIND: Mutex<RefCell<Option<state::PageKind>>> = Mutex::new(RefCell::new(None));
static LAST_OMNI_TRANSFORM_ACTIVE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
static LAST_TRANSFORM_ACTIVE: Mutex<RefCell<bool>> = Mutex::new(RefCell::new(false));
static LAST_SETTINGS_STATE: Mutex<RefCell<Option<SettingsMenuState>>> =
    Mutex::new(RefCell::new(None));

// Clear all cached assets and state (call after waking from deep sleep)
pub fn clear_all_caches() {
    assets::clear_cache();
    brightness::reset_flags();
    time::reset_clock_state();
    pages::watch::reset_on_exit();
    pages::uart_terminal::reset_on_exit();
    crate::app::state::terminal_reset();
    state::clear_nav();
    critical_section::with(|cs| {
        *LAST_PAGE_KIND.borrow(cs).borrow_mut() = None;
        *LAST_OMNI_TRANSFORM_ACTIVE.borrow(cs).borrow_mut() = false;
        *LAST_TRANSFORM_ACTIVE.borrow(cs).borrow_mut() = false;
        *LAST_SETTINGS_STATE.borrow(cs).borrow_mut() = None;
    });
}

// helper function to update the display based on UI_STATE
pub fn update_ui(disp: &mut impl PanelRgb565, state: UiState, redraw: bool) {
    // If caller does not want a redraw this cycle, bail out early.
    if !redraw {
        return;
    }
    // Clear when:
    // entering Omnitrix from another page, OR
    // exiting Transform dialog while staying in Omnitrix
    let current_kind = match state.page {
        Page::UartTerminal => state::PageKind::UartTerminal,
        Page::Main(_) => state::PageKind::Main,
        Page::Settings(_) => state::PageKind::Settings,
        Page::Omnitrix(_) => state::PageKind::Omnitrix,
        Page::Games(_) => state::PageKind::Games,
        Page::EasterEgg => state::PageKind::EasterEgg,
        Page::Watch(_) => state::PageKind::Watch,
    };

    // Check if are currently in the Transform dialog on Omnitrix page
    let current_transform_active = matches!(state.page, Page::Omnitrix(_))
        && matches!(state.dialog, Some(Dialog::TransformPage));

    // Determine if should clear without FB update
    let should_clear_no_fb = critical_section::with(|cs| {
        let mut last_kind = LAST_PAGE_KIND.borrow(cs).borrow_mut();
        let mut last_tx = LAST_OMNI_TRANSFORM_ACTIVE.borrow(cs).borrow_mut();

        let entering_omni = current_kind == state::PageKind::Omnitrix
            && *last_kind != Some(state::PageKind::Omnitrix);
        let exiting_transform =
            (*last_tx) && current_kind == state::PageKind::Omnitrix && !current_transform_active;

        // update trackers for next frame
        *last_kind = Some(current_kind);
        *last_tx = current_transform_active;

        entering_omni || exiting_transform
    });

    // Clear display if needed
    if should_clear_no_fb {
        let _ = if let Some(co) =
            (disp as &mut dyn Any).downcast_mut::<crate::display::DisplayType<'static>>()
        {
            let _ = crate::display::FastPanelOps::fill_rect_solid_no_fb(
                co,
                0,
                0,
                RESOLUTION as u16,
                RESOLUTION as u16,
                Rgb565::BLACK,
            );
        } else {
            // Fallback to normal clear
            disp.clear(Rgb565::BLACK).ok();
        };
    }

    // If a dialog is active, render it and return early.
    if let Some(dialog) = state.dialog {
        match dialog {
            Dialog::TransformPage => {
                let mut active =
                    critical_section::with(|cs| *LAST_TRANSFORM_ACTIVE.borrow(cs).borrow());
                pages::transform::render(disp, &mut active);
                critical_section::with(|cs| {
                    *LAST_TRANSFORM_ACTIVE.borrow(cs).borrow_mut() = active;
                });
            }
        }
        return;
    }

    // Reset watch-state tracker if we’re not on the Watch page.
    if !matches!(state.page, Page::Watch(_)) {
        pages::watch::reset_on_exit();
    }
    if !matches!(state.page, Page::Games(GamesState::Pong)) {
        pong_reset_on_exit();
    }
    if !matches!(state.page, Page::Games(GamesState::Snake)) {
        snake_reset_on_exit();
    }
    if !matches!(state.page, Page::UartTerminal) {
        pages::uart_terminal::reset_on_exit();
    }
    if !matches!(state.page, Page::Games(_)) {
        assets::clear_cached_asset(AssetId::PongIcon);
        assets::clear_cached_asset(AssetId::SnakeIcon);
    }

    // Handle entering/exiting brightness adjustment mode
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

    // If leaving settings page, reset brightness edit mode and last pct.
    if !matches!(state.page, Page::Settings(_)) {
        brightness_edit_set(false);
        brightness::reset_brightness_last();
    } else if entering_brightness {
        brightness::reset_brightness_last();
    }
    // Reset transform tracker when dialog is not active.
    critical_section::with(|cs| {
        *LAST_TRANSFORM_ACTIVE.borrow(cs).borrow_mut() = false;
    });

    // Dispatch to page renderers
    match state.page {
        Page::UartTerminal => pages::uart_terminal::render(disp),
        Page::Main(menu_state) => pages::main_menu::render(disp, menu_state),
        Page::Settings(settings_state) => pages::settings::render(disp, settings_state),
        Page::Watch(watch_state) => pages::watch::render(disp, watch_state),
        Page::Omnitrix(omnitrix_state) => pages::omnitrix::render(disp, omnitrix_state),
        Page::Games(games_state) => pages::games::render(disp, games_state),
        Page::EasterEgg => pages::easter_egg::render(disp),
    }
}
