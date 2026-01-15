// Application runtime loop

use crate::app::{controller, state};
use crate::drivers::qmi8658_imu::{Qmi8658, SmashDetector};
use crate::drivers::rtc_pcf85063::{unix_to_datetime, Pcf85063};
use crate::hal;
use crate::ui::{self, Dialog, GamesState, Page, SettingsMenuState, UiState, WatchAppState};
use crate::{display, hal::bus::SharedI2cDev, hal::bus::SharedI2cRef};
use esp_hal::rtc_cntl::Rtc;
use esp_hal::timer::systimer::{SystemTimer, Unit};

pub const DEBOUNCE_MS: u64 = 150;

// Rotary encoder detent steps
pub struct RunConfig {
    pub detent_steps: i32,
    pub sleep_hold_ms: u64,
}

// Main application runtime loop
pub fn run(
    display: &mut display::DisplayType<'static>,
    rtc: &mut Rtc<'_>,
    rtc_boot_time_us: u64,
    rtc_bus: Option<SharedI2cRef>,
    imu: &mut Option<Qmi8658<SharedI2cDev>>,
    apply_brightness: Option<fn(&mut display::DisplayType<'static>, u8)>,
    woke_from_sleep: bool,
    config: RunConfig,
) -> ! {
    let mut last_ui_state = state::ui_state_get();
    let mut needs_redraw = true;
    let mut last_detent: Option<i32> = None;
    let mut sleep_hold_start: Option<u64> = None;
    let mut last_watch_edit_active = false;
    let mut last_pong_back_ms: Option<u64> = None;

    let mut smash_detector = SmashDetector::default_rough();
    let mut last_sample: Option<crate::drivers::qmi8658_imu::ImuSample> = None;
    let mut next_poll_ms: u64 = 0;
    let mut smash_count: u8 = 0;

    if woke_from_sleep {
        hal::sleep::wait_for_wake_button_release(
            state::button2(),
            state::button1_flag(),
            state::button2_flag(),
        );
    }

    loop {
        let now_ms = {
            let t = SystemTimer::unit_value(Unit::Unit0);
            t.saturating_mul(1000) / SystemTimer::ticks_per_second()
        };

        // Check for UI state changes
        let ui_state = state::ui_state_get();
        if ui_state != last_ui_state {
            last_ui_state = ui_state;
            needs_redraw = true;
        }
        let in_omnitrix = matches!(ui_state.page, Page::Omnitrix(_));
        let in_pong = matches!(ui_state.page, Page::Games(GamesState::Pong));
        if !in_pong {
            last_pong_back_ms = None;
        }
        if !in_omnitrix {
            smash_count = 0;
        }

        if matches!(ui_state.page, Page::Watch(WatchAppState::Digital))
            || matches!(ui_state.page, Page::Watch(WatchAppState::Analog))
        {
            // Keep redrawing to refresh the clock hands/digits while in watch modes.
            needs_redraw = true;
        }

        if matches!(
            ui_state.page,
            Page::Settings(SettingsMenuState::BrightnessAdjust)
        ) {
            if ui::brightness_take_dirty() {
                needs_redraw = true;
            }
        }
        if in_pong && ui::pong_ball_active() {
            if ui::pong_ball_update(
                now_ms,
                ui::pong_play_radius(),
                ui::pong_paddle_angle(),
                ui::CENTER,
                ui::CENTER,
            ) {
                needs_redraw = true;
            }
        }

        // Keep redrawing while the Transform dialog is visible so the helix animates.
        if matches!(ui_state.dialog, Some(Dialog::TransformPage)) {
            needs_redraw = true;
        }

        ui::update_ui(display, last_ui_state, needs_redraw);
        needs_redraw = false;

        // IMU smash detection
        if let Some(dev) = imu.as_mut() {
            // Only read when IMU INT fired, additional fall back to periodic reads if INT never comes.
            let timed = now_ms >= next_poll_ms;
            let pin_level_trig = state::imu_int_pin_low();
            let should_read =
                state::take_imu_int_flag() || pin_level_trig || last_sample.is_none() || timed;
            if should_read {
                // Read sample
                match dev.read_sample() {
                    Ok(sample) => {
                        // Process sample for smash detection
                        if smash_detector.update(now_ms, &sample) {
                            // the omnitrix page is the only one that uses this input
                            if in_omnitrix {
                                smash_count = smash_count.saturating_add(1);
                                // smashes (amount is adjustable)
                                if smash_count >= 1 {
                                    // reset count after triggering
                                    smash_count = 0;
                                    state::button3_flag()
                                        .store(true, core::sync::atomic::Ordering::Relaxed);
                                }
                            }
                        }
                        last_sample = Some(sample);
                    }
                    Err(_e) => {}
                }

                if timed {
                    next_poll_ms = now_ms.saturating_add(50);
                }
            }
        }

        // Handle button events
        let b1_event = state::take_button1_event();
        let b2_event = state::take_button2_event();
        let b3_event = state::take_button3_event();

        hal::sleep::handle_deep_sleep(
            now_ms,
            &mut sleep_hold_start,
            rtc,
            rtc_boot_time_us,
            display,
            state::button1(),
            state::button2(),
            config.sleep_hold_ms,
        );

        if b1_event {
            if in_pong {
                let now = now_ms;
                let quick_ms = 450;
                if let Some(last_ms) = last_pong_back_ms {
                    if now.saturating_sub(last_ms) <= quick_ms {
                        last_pong_back_ms = None;
                        needs_redraw |= controller::handle_back();
                    } else {
                        last_pong_back_ms = Some(now);
                    }
                } else {
                    last_pong_back_ms = Some(now);
                }
            } else {
                needs_redraw |= controller::handle_back();
            }
        }

        if b2_event {
            if in_pong {
                if ui::pong_ball_active() {
                    if ui::pong_paddle_flip_timed(now_ms) {
                        needs_redraw = true;
                    }
                } else if ui::pong_ball_start(
                    now_ms,
                    ui::pong_paddle_angle(),
                    ui::pong_play_radius(),
                ) {
                    needs_redraw = true;
                }
            } else {
                needs_redraw |= controller::handle_select();
            }
        }

        if b3_event {
            if controller::handle_transform(in_omnitrix) {
                needs_redraw = true;
            }
        }

        // Rotary encoder handling
        let pos = state::rotary_position();
        let detent = pos / config.detent_steps; // use division (works well for negatives too)

        // If detent changed, update UI state
        if Some(detent) != last_detent {
            if let Some(prev) = last_detent {
                let step_delta = detent - prev;
                let ui_state = state::ui_state_get();
                if ui::watch_edit_active() {
                    ui::watch_edit_adjust(-step_delta);
                    needs_redraw = true;
                } else if matches!(
                    ui_state.page,
                    Page::Settings(SettingsMenuState::BrightnessAdjust)
                ) {
                    let new_pct = ui::brightness_adjust(-step_delta);
                    if let Some(apply) = apply_brightness {
                        apply(display, new_pct);
                    }
                    needs_redraw = true;
                } else if matches!(ui_state.page, Page::Games(GamesState::Pong)) {
                    if ui::pong_paddle_adjust_timed(-step_delta, now_ms) {
                        needs_redraw = true;
                    }
                } else if step_delta > 0 {
                    state::ui_state_update(UiState::prev_item);
                    needs_redraw = true;
                } else if step_delta < 0 {
                    state::ui_state_update(UiState::next_item);
                    needs_redraw = true;
                }
            }
            last_detent = Some(detent);
        }

        // If we just exited watch edit, sync external RTC with current software clock.
        let edit_active = ui::watch_edit_active();
        if last_watch_edit_active && !edit_active {
            if let Some(bus_ref) = rtc_bus {
                let dev = embedded_hal_bus::i2c::RefCellDevice::new(bus_ref);
                let mut rtc_handle = Pcf85063::new(dev);
                let secs = ui::clock_now_seconds_u32();
                let dt = unix_to_datetime(secs);
                let _ = rtc_handle.set_datetime(&dt);
            }
        }
        last_watch_edit_active = edit_active;
    }
}
