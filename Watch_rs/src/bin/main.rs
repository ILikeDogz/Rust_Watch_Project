//% CHIPS: esp32s3
//% FEATURES: esp-hal/unstable

#![no_std]
#![no_main]

// Define the application description, which is placed in a special section of the binary.
// This is used by the bootloader to verify the application.
// The macro automatically fills in the fields.
esp_bootloader_esp_idf::esp_app_desc!();

// Module imports
use esp32s3_tests::{
    app, init,
    wiring::{init_board_pins, BoardPins},
    display::init_display,
};

// Core imports
use esp_backtrace as _;

// ESP-HAL imports
use esp_hal::{
    main, psram,
    timer::systimer::{SystemTimer, Unit},
    Config,
};

#[cfg(feature = "esp32s3-disp143Oled")]
fn apply_brightness(display: &mut esp32s3_tests::display::DisplayType<'static>, pct: u8) {
    let hw = ((pct as u16) * 255 / 100) as u8;
    let _ = display.set_brightness(hw);
}
const SLEEP_HOLD_MS: u64 = 5000; // Hold button 1 for 5 seconds to sleep/wake

// Interrupt handler is in app::interrupts

#[main]
fn main() -> ! {
    // initial UI state lives in app::state

    // Initialize peripherals
    let peripherals = esp_hal::init(Config::default());

    // Initialize PSRAM allocator
    esp_alloc::psram_allocator!(&peripherals.PSRAM, psram);

    // one call gives you IO handler + all your role pins from wiring.rs
    let (mut io, pins, i2c0) = init_board_pins(peripherals);

    // Destructure pins for easier access
    let BoardPins {
        btn1,
        btn2,
        btn3,
        enc_clk,
        enc_dt,
        #[cfg(feature = "esp32s3-disp143Oled")]
        imu_int,
        display_pins,
        #[cfg(feature = "esp32s3-disp143Oled")]
        imu_i2c,
        #[cfg(feature = "esp32s3-disp143Oled")]
        lpwr,
    } = pins;

    // -------------------- RTC and Deep Sleep Wake Detection --------------------
    #[cfg(feature = "esp32s3-disp143Oled")]
    let (mut rtc, rtc_boot_time_us, woke_from_sleep) = init::rtc::init_rtc_and_wake(lpwr);

    // rotary encoder detent tracking
    const DETENT_STEPS: i32 = 4; // set to 4 if your encoder is 4 steps per detent
    #[cfg(feature = "esp32s3-disp143Oled")]
    let imu_int_opt = Some(imu_int);
    #[cfg(not(feature = "esp32s3-disp143Oled"))]
    let imu_int_opt = None;

    // Install inputs
    app::state::install_inputs(btn1, btn2, btn3, enc_clk, enc_dt, imu_int_opt);

    // Set up interrupt handler
    io.set_interrupt_handler(app::interrupts::handler);

    let mut my_display = init_display(display_pins);

    // -------------------- IMU and RTC initialization --------------------

    // Initialize IMU and RTC I2C bus
    let rtc_bus = init::i2c::init_shared_i2c_bus(i2c0, imu_i2c);
    let rtc_secs = rtc_bus.and_then(init::rtc::read_rtc_seconds);
    let boot_secs = rtc_secs.unwrap_or_else(|| {
        let now = SystemTimer::unit_value(Unit::Unit0);
        (now / SystemTimer::ticks_per_second()) as u32
    });
    esp32s3_tests::ui::set_clock_seconds(boot_secs);
    let mut imu = rtc_bus.map(init::imu::init_imu).flatten();

    // count smash gestures while on Omnitrix page

    // // -------------------- UI Init --------------------

    #[cfg(feature = "esp32s3-disp143Oled")]
    {
        app::boot::run_boot_sequence(&mut my_display);
    }

    // -------------------- Demo Sequence --------------------
    // // Demo sequence timing (for display driver benchmarking)
    // let demo_start_ms = {
    //     let t = SystemTimer::unit_value(Unit::Unit0);
    //     t.saturating_mul(1000) / SystemTimer::ticks_per_second()
    // };

    // // Helper: ticks -> microseconds
    // let ticks_per_s = SystemTimer::ticks_per_second() as u64;
    // let to_us = |t0: u64, t1: u64| -> u64 {
    //     let dt = t1.saturating_sub(t0);
    //     dt.saturating_mul(1_000_000) / ticks_per_s
    // };

    // enum DemoState {
    //     Home,
    //     Omnitrix,
    //     Rotating { idx: usize, last_ms: u64 },
    //     BackToHome { last_ms: u64 },
    //     Done,
    // }

    // let mut demo_state = DemoState::Home;

    // loop {
    //     let now_ms = {
    //         let t = SystemTimer::unit_value(Unit::Unit0);
    //         t.saturating_mul(1000) / SystemTimer::ticks_per_second()
    //     };
    //     let rel = now_ms - demo_start_ms; // relative elapsed

    //     match demo_state {
    //         DemoState::Home => {
    //             if rel > 1000 {
    //                 critical_section::with(|cs| {
    //                     UI_STATE.borrow(cs).set(UiState {
    //                         page: Page::Omnitrix(esp32s3_tests::ui::OmnitrixState::Alien1),
    //                         dialog: None,
    //                     });
    //                 });
    //                 demo_state = DemoState::Omnitrix;
    //             }
    //         }
    //         DemoState::Omnitrix => {
    //             if rel > 1500 {
    //                 demo_state = DemoState::Rotating {
    //                     idx: 1,
    //                     last_ms: rel,
    //                 };
    //             }
    //         }
    //         DemoState::Rotating { idx, last_ms } => {
    //             // Rotate every 3000 ms (comment and code agree now)
    //             if rel > last_ms + 3000 {
    //                 let next_state = match idx {
    //                     1 => esp32s3_tests::ui::OmnitrixState::Alien2,
    //                     2 => esp32s3_tests::ui::OmnitrixState::Alien3,
    //                     3 => esp32s3_tests::ui::OmnitrixState::Alien4,
    //                     4 => esp32s3_tests::ui::OmnitrixState::Alien5,
    //                     5 => esp32s3_tests::ui::OmnitrixState::Alien6,
    //                     6 => esp32s3_tests::ui::OmnitrixState::Alien7,
    //                     7 => esp32s3_tests::ui::OmnitrixState::Alien8,
    //                     8 => esp32s3_tests::ui::OmnitrixState::Alien9,
    //                     9 => esp32s3_tests::ui::OmnitrixState::Alien10,
    //                     _ => esp32s3_tests::ui::OmnitrixState::Alien1,
    //                 };
    //                 critical_section::with(|cs| {
    //                     UI_STATE.borrow(cs).set(UiState {
    //                         page: Page::Omnitrix(next_state),
    //                         dialog: None,
    //                     });
    //                 });
    //                 if idx < 9 {
    //                     demo_state = DemoState::Rotating {
    //                         idx: idx + 1,
    //                         last_ms: rel,
    //                     };
    //                 } else {
    //                     demo_state = DemoState::BackToHome { last_ms: rel };
    //                 }
    //             }
    //         }
    //         DemoState::BackToHome { last_ms } => {
    //             if rel > last_ms + 1500 {
    //                 // critical_section::with(|cs| {
    //                 //     UI_STATE.borrow(cs).set(UiState { page: Page::Main(MainMenuState::Home), dialog: None });
    //                 // });

    //                 // set to info page instead
    //                 critical_section::with(|cs| {
    //                     UI_STATE.borrow(cs).set(UiState {
    //                         page: Page::Info,
    //                         dialog: None,
    //                     });
    //                 });
    //                 demo_state = DemoState::Done;
    //             }
    //         }
    //         DemoState::Done => { /* stop or loop again */ }
    //     }

    //     let ui_state = critical_section::with(|cs| UI_STATE.borrow(cs).get());
    //     if ui_state != last_ui_state {
    //         last_ui_state = ui_state;
    //         needs_redraw = true;
    //     }
    //     let t0 = SystemTimer::unit_value(Unit::Unit0);
    //     update_ui(&mut my_display, last_ui_state, needs_redraw);
    //     if needs_redraw {
    //         let t1 = SystemTimer::unit_value(Unit::Unit0);
    //         esp_println::println!("UI update: {} us", to_us(t0, t1));
    //     }
    //     needs_redraw = false;

    //     for _ in 0..10000 {
    //         core::hint::spin_loop();
    //     }
    // }

    // // -------------------- Main loop --------------------

    // Configuration for runtime
    let config = app::runtime::RunConfig {
        detent_steps: DETENT_STEPS,
        sleep_hold_ms: SLEEP_HOLD_MS,
    };

    #[cfg(feature = "esp32s3-disp143Oled")]
    let apply = Some(apply_brightness as fn(&mut esp32s3_tests::display::DisplayType<'static>, u8));
    #[cfg(not(feature = "esp32s3-disp143Oled"))]
    let apply = None;

    // Enter main application runtime loop
    app::runtime::run(
        &mut my_display,
        &mut rtc,
        rtc_boot_time_us,
        rtc_bus,
        &mut imu,
        apply,
        woke_from_sleep,
        config,
    );
}
