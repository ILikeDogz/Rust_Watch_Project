// Functions for handling deep sleep and wake behavior

use crate::input::ButtonState;
use crate::ui::get_clock_seconds;
use esp_hal::rtc_cntl::{
    sleep::{Ext0WakeupSource, WakeupLevel},
    Rtc,
};

use core::sync::atomic::AtomicBool;
use embedded_hal::delay::DelayNs;

use crate::hal::timer::TimerDelay;

const WAKE_RELEASE_POLL_MS: u32 = 10;
const WAKE_RELEASE_TIMEOUT_MS: u32 = 3000;
const POST_RELEASE_DELAY_MS: u32 = 50;

// Wait for the wake button (Button 2) to be released after waking from deep sleep
pub fn wait_for_wake_button_release(
    btn2: &ButtonState<'static>,
    btn1_pressed: &AtomicBool,
    btn2_pressed: &AtomicBool,
) {
    // If woke from deep sleep, wait for the wake button (Button 2) to be released
    // This prevents the wake press from being registered as a UI action
    let mut delay = TimerDelay;
    wait_for_button_release(btn2, &mut delay);
    btn1_pressed.store(false, core::sync::atomic::Ordering::Release);
    btn2_pressed.store(false, core::sync::atomic::Ordering::Release);
}

// Handle deep sleep entry when Button 1 is held for a specified duration
pub fn handle_deep_sleep(
    now_ms: u64,
    sleep_hold_start: &mut Option<u64>,
    rtc: &mut Rtc<'_>,
    rtc_boot_time_us: u64,
    display: &mut crate::display::DisplayType<'static>,
    btn1: &ButtonState<'static>,
    btn2: &ButtonState<'static>,
    sleep_hold_ms: u64,
) {
    // Track button 1 hold for deep sleep trigger
    let btn1_down = is_button_low(btn1);

    // Start tracking hold when button 1 goes down
    if btn1_down && sleep_hold_start.is_none() {
        *sleep_hold_start = Some(now_ms);
    }
    // Reset if button released
    if !btn1_down {
        *sleep_hold_start = None;
    }

    // Check for 5-second hold to enter deep sleep
    if let Some(t0) = *sleep_hold_start {
        if now_ms.saturating_sub(t0) >= sleep_hold_ms && btn1_down {
            save_clock_to_rtc(rtc, rtc_boot_time_us);
            disable_display(display);
            wait_for_button_release(btn1, &mut TimerDelay);
            release_button_pins(btn1, btn2);
            enter_deep_sleep(rtc);
        }
    }
}

fn is_button_low(btn: &ButtonState<'static>) -> bool {
    critical_section::with(|cs| {
        btn.input
            .borrow_ref(cs)
            .as_ref()
            .map(|p| p.is_low())
            .unwrap_or(false)
    })
}

fn wait_for_button_release(btn: &ButtonState<'static>, delay: &mut TimerDelay) {
    let mut wait_ms = 0u32;
    loop {
        let released = critical_section::with(|cs| {
            btn.input
                .borrow_ref(cs)
                .as_ref()
                .map(|b| b.is_high())
                .unwrap_or(true)
        });
        if released {
            break;
        }
        delay.delay_ms(WAKE_RELEASE_POLL_MS);
        wait_ms = wait_ms.saturating_add(WAKE_RELEASE_POLL_MS);
        if wait_ms >= WAKE_RELEASE_TIMEOUT_MS {
            break;
        }
    }
    delay.delay_ms(POST_RELEASE_DELAY_MS);
}

fn save_clock_to_rtc(rtc: &mut Rtc<'_>, rtc_boot_time_us: u64) {
    let current_clock_secs = get_clock_seconds();
    let rtc_now_us = rtc.current_time_us();
    let elapsed_since_boot_us = rtc_now_us.saturating_sub(rtc_boot_time_us);
    let clock_total_us =
        (current_clock_secs as u64) * 1_000_000 + (elapsed_since_boot_us % 1_000_000);
    rtc.set_current_time_us(clock_total_us);
}

fn disable_display(display: &mut crate::display::DisplayType<'static>) {
    let mut delay = TimerDelay;
    let _ = display.disable(&mut delay);
}

fn release_button_pins(btn1: &ButtonState<'static>, btn2: &ButtonState<'static>) {
    critical_section::with(|cs| {
        let _ = btn1.input.borrow_ref_mut(cs).take();
        let _ = btn2.input.borrow_ref_mut(cs).take();
    });
}

fn enter_deep_sleep(rtc: &mut Rtc<'_>) {
    // Configure GPIO7 (Button 2) as wake source with RTC pull-up
    // uses unsafe steal since we've released the pin from earlier
    let gpio7 = unsafe { esp_hal::peripherals::GPIO7::steal() };
    use esp_hal::gpio::RtcPinWithResistors;
    gpio7.rtcio_pullup(true);
    gpio7.rtcio_pulldown(false);
    let ext0_wake = Ext0WakeupSource::new(gpio7, WakeupLevel::Low);
    rtc.sleep_deep(&[&ext0_wake]);
}
