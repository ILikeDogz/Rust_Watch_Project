//! Watch Prototype
//! ========================================
//! needs to be run in WSL2 terminal
//! source ~/export-esp.sh
//! ========================================
//!
//! Toggles LCD when either button pressed.
//! It also reads a rotary encoder and toggles LCD.

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
    display::setup_display,
    input::{
        handle_button_generic, handle_encoder_generic, handle_imu_int_generic, ButtonState,
        ImuIntState, RotaryState,
    },
    qmi8658_imu::{Qmi8658, SmashDetector, DEFAULT_I2C_ADDR},
    ui::{
        brightness_adjust, clear_all_caches, clock_now_seconds_u32, get_clock_seconds,
        precache_asset, set_clock_seconds, update_ui, AssetId, Dialog, MainMenuState, Page,
        SettingsMenuState, UiState, WatchAppState,
    },
    wiring::{init_board_pins, BoardPins},
};

use esp32s3_tests::rtc_pcf85063::{datetime_to_unix, unix_to_datetime, Pcf85063};

#[cfg(feature = "esp32s3-disp143Oled")]
use esp32s3_tests::display::TimerDelay;

// Core imports
use core::cell::{Cell, RefCell};
use critical_section::Mutex;
use esp_backtrace as _;

// ESP-HAL imports
use esp_hal::{
    handler,
    i2c::master::{Config as I2cConfig, I2c},
    main, psram, ram,
    rtc_cntl::{
        reset_reason,
        sleep::{Ext0WakeupSource, WakeupLevel},
        wakeup_cause, Rtc, SocResetReason,
    },
    system::Cpu,
    time::Rate,
    timer::systimer::{SystemTimer, Unit},
    Config,
};

// Embedded HAL trait for delay
use embedded_hal::delay::DelayNs;
use embedded_hal::i2c::I2c as _;

#[cfg(feature = "esp32s3-disp143Oled")]
// Println macro
use esp_println::println;

// Allocator for PSRAM
extern crate alloc;
use alloc::{boxed::Box, vec};

#[cfg(feature = "devkit-esp32s3-disp128")]
#[ram]
static mut DISPLAY_BUF: [u8; 1024] = [0; 1024];

use core::sync::atomic::{AtomicBool, Ordering};
static BUTTON1_PRESSED: AtomicBool = AtomicBool::new(false);
static BUTTON2_PRESSED: AtomicBool = AtomicBool::new(false);
static BUTTON3_PRESSED: AtomicBool = AtomicBool::new(false);
static IMU_INT_FLAG: AtomicBool = AtomicBool::new(false);

// Shared resources for Button
static BUTTON1: ButtonState<'static> = ButtonState {
    input: Mutex::new(RefCell::new(None)),
    // led: Mutex::new(RefCell::new(None)),
    last_level: Mutex::new(Cell::new(true)),
    last_interrupt: Mutex::new(Cell::new(0)),
    name: "Button1",
};

static BUTTON2: ButtonState<'static> = ButtonState {
    input: Mutex::new(RefCell::new(None)),
    // led: Mutex::new(RefCell::new(None)),
    last_level: Mutex::new(Cell::new(true)),
    last_interrupt: Mutex::new(Cell::new(0)),
    name: "Button2",
};

static BUTTON3: ButtonState<'static> = ButtonState {
    input: Mutex::new(RefCell::new(None)),
    // led: Mutex::new(RefCell::new(None)),
    last_level: Mutex::new(Cell::new(true)),
    last_interrupt: Mutex::new(Cell::new(0)),
    name: "Button3",
};

// Shared resources for rotary encoder
static ROTARY: RotaryState<'static> = RotaryState {
    clk: Mutex::new(RefCell::new(None)),
    dt: Mutex::new(RefCell::new(None)),
    position: Mutex::new(Cell::new(0)),
    last_qstate: Mutex::new(Cell::new(0)), // bits: [CLK<<1 | DT]
    last_step: Mutex::new(Cell::new(0)),   // +1 or -1 from last transition
};

#[cfg(feature = "esp32s3-disp143Oled")]
fn apply_brightness(display: &mut esp32s3_tests::display::DisplayType<'static>, pct: u8) {
    let hw = ((pct as u16) * 255 / 100) as u8;
    let _ = display.set_brightness(hw);
}

// Global UI state
static UI_STATE: Mutex<Cell<UiState>> = Mutex::new(Cell::new(UiState {
    page: Page::Main(MainMenuState::Home),
    // page: Page::Omnitrix(esp32s3_tests::ui::OmnitrixState::Alien1),
    dialog: None,
}));

// IMU interrupt input holder
#[cfg(feature = "esp32s3-disp143Oled")]
static IMU_INT: ImuIntState<'static> = ImuIntState {
    input: Mutex::new(RefCell::new(None)),
};

// Current debounce time (milliseconds)
const DEBOUNCE_MS: u64 = 240;
const SLEEP_HOLD_MS: u64 = 5000; // Hold button 1 for 5 seconds to sleep/wake

// Interrupt handler
#[handler]
#[ram]
fn handler() {
    let now_ms = {
        let t = SystemTimer::unit_value(Unit::Unit0);
        t.saturating_mul(1000) / SystemTimer::ticks_per_second()
    };

    // Button 1: JUST SET THE FLAG
    handle_button_generic(&BUTTON1, now_ms, DEBOUNCE_MS, || {
        BUTTON1_PRESSED.store(true, Ordering::Relaxed);
    });

    // Button 2: JUST SET THEFlag
    handle_button_generic(&BUTTON2, now_ms, DEBOUNCE_MS, || {
        BUTTON2_PRESSED.store(true, Ordering::Relaxed);
    });

    // Button 3: JUST SET THE FLAG
    handle_button_generic(&BUTTON3, now_ms, DEBOUNCE_MS, || {
        BUTTON3_PRESSED.store(true, Ordering::Relaxed);
    });

    // Encoder logic is fine, it's just math
    handle_encoder_generic(&ROTARY);

    #[cfg(feature = "esp32s3-disp143Oled")]
    {
        handle_imu_int_generic(&IMU_INT, &IMU_INT_FLAG);
    }
}

#[main]
fn main() -> ! {
    // initial UI state
    let mut last_ui_state = UiState {
        page: Page::Main(MainMenuState::Home),
        dialog: None,
    };

    let mut needs_redraw = true;
    // // for debug, start on Alien page
    // let mut last_ui_state = UiState { page: Page::Omnitrix(esp32s3_tests::ui::OmnitrixState::Alien10), dialog: None };

    // Initialize peripherals
    let peripherals = esp_hal::init(Config::default());

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
    let mut rtc = Rtc::new(lpwr);

    // Track the RTC time when we booted/woke, so we can calculate elapsed time
    #[cfg(feature = "esp32s3-disp143Oled")]
    let rtc_boot_time_us: u64 = rtc.current_time_us();

    #[cfg(feature = "esp32s3-disp143Oled")]
    let woke_from_sleep = {
        let reason = reset_reason(Cpu::ProCpu).unwrap_or(SocResetReason::ChipPowerOn);
        let wake = wakeup_cause();

        // Check if waking from deep sleep
        // After deep sleep, the RTC timer continues but everything else resets
        let from_sleep = matches!(reason, SocResetReason::CoreDeepSleep)
            || matches!(
                wake,
                esp_hal::system::SleepSource::Gpio
                    | esp_hal::system::SleepSource::Ext0
                    | esp_hal::system::SleepSource::Ext1
                    | esp_hal::system::SleepSource::Timer
            );

        if from_sleep {
            // RTC kept running during sleep - restore clock from RTC value
            let restored_secs = (rtc_boot_time_us / 1_000_000) as u32;
            set_clock_seconds(restored_secs);
            clear_all_caches();
        }
        from_sleep
    };

    // rotary encoder detent tracking
    const DETENT_STEPS: i32 = 4; // set to 4 if your encoder is 4 steps per detent
    let mut last_detent: Option<i32> = None;
    let mut sleep_hold_start: Option<u64> = None; // Track button 1 hold for deep sleep
    let mut last_watch_edit_active = false;

    // Read encoder pin states BEFORE moving them
    let clk_initial = enc_clk.is_high() as u8;
    let dt_initial = enc_dt.is_high() as u8;
    let qstate_initial = (clk_initial << 1) | dt_initial;

    // Stash pins in global state
    critical_section::with(|cs| {
        BUTTON1.input.borrow_ref_mut(cs).replace(btn1);
        BUTTON1.last_level.borrow(cs).set(true);

        BUTTON2.input.borrow_ref_mut(cs).replace(btn2);
        BUTTON2.last_level.borrow(cs).set(true);

        BUTTON3.input.borrow_ref_mut(cs).replace(btn3);
        BUTTON3.last_level.borrow(cs).set(true);

        ROTARY.clk.borrow_ref_mut(cs).replace(enc_clk);
        ROTARY.dt.borrow_ref_mut(cs).replace(enc_dt);
        ROTARY.last_qstate.borrow(cs).set(qstate_initial);
        ROTARY.position.borrow(cs).set(0);
        ROTARY.last_step.borrow(cs).set(0);

        #[cfg(feature = "esp32s3-disp143Oled")]
        IMU_INT.input.borrow_ref_mut(cs).replace(imu_int);
    });

    // If we woke from deep sleep, wait for the wake button (Button 2) to be released
    // This prevents the wake press from being registered as a UI action
    #[cfg(feature = "esp32s3-disp143Oled")]
    if woke_from_sleep {
        let mut delay = TimerDelay;
        let mut wait_count = 0u32;
        loop {
            let btn2_released = critical_section::with(|cs| {
                BUTTON2
                    .input
                    .borrow_ref(cs)
                    .as_ref()
                    .map(|b| b.is_high())
                    .unwrap_or(true)
            });
            if btn2_released {
                break;
            }
            delay.delay_ms(10);
            wait_count += 1;
            // Timeout after 3 seconds
            if wait_count > 300 {
                break;
            }
        }
        delay.delay_ms(50);
        BUTTON1_PRESSED.store(false, Ordering::Release);
        BUTTON2_PRESSED.store(false, Ordering::Release);
    }

    io.set_interrupt_handler(handler);

    let mut my_display = {
        #[cfg(feature = "devkit-esp32s3-disp128")]
        {
            // Safe because DISPLAY_BUF is only used here
            unsafe { setup_display(display_pins, &mut DISPLAY_BUF) }
        }

        #[cfg(feature = "esp32s3-disp143Oled")]
        {
            const W: usize = 466;
            let fb: &'static mut [u16] = Box::leak(vec![0u16; W * W].into_boxed_slice());

            setup_display(display_pins, fb)
        }
    };

    // -------------------- IMU and RTC initialization --------------------

    #[cfg(feature = "esp32s3-disp143Oled")]
    let mut rtc_bus: Option<&'static core::cell::RefCell<I2c<'static, esp_hal::Blocking>>> = None;
    #[cfg(feature = "esp32s3-disp143Oled")]
    let mut imu = {
        let cfg = I2cConfig::default().with_frequency(Rate::from_khz(400));
        match I2c::new(i2c0, cfg) {
            Ok(i2c) => {
                let i2c = i2c.with_sda(imu_i2c.sda).with_scl(imu_i2c.scl);
                let bus = core::cell::RefCell::new(i2c);
                let bus_static: &'static core::cell::RefCell<I2c<'static, esp_hal::Blocking>> =
                    Box::leak(Box::new(bus));
                let rtc_dev = embedded_hal_bus::i2c::RefCellDevice::new(bus_static);
                let mut rtc_handle = Pcf85063::new(rtc_dev);
                let rtc_secs = rtc_handle.read_datetime().ok().and_then(|(dt, vl)| {
                    if vl {
                        // esp_println::println!(
                        //     "[RTC] VL=1 dt={:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                        //     dt.year,
                        //     dt.month,
                        //     dt.day,
                        //     dt.hour,
                        //     dt.minute,
                        //     dt.second
                        // );
                        None
                    } else {
                        // esp_println::println!(
                        //     "[RTC] read ok {:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                        //     dt.year,
                        //     dt.month,
                        //     dt.day,
                        //     dt.hour,
                        //     dt.minute,
                        //     dt.second
                        // );
                        Some(datetime_to_unix(&dt))
                    }
                });
                let boot_secs = rtc_secs.unwrap_or_else(|| {
                    let now = SystemTimer::unit_value(Unit::Unit0);
                    (now / SystemTimer::ticks_per_second()) as u32
                });
                // esp_println::println!("[RTC] boot set_clock_seconds({})", boot_secs);
                set_clock_seconds(boot_secs);
                rtc_bus = Some(bus_static);
                let mut bus_device = embedded_hal_bus::i2c::RefCellDevice::new(bus_static);

                // Small helper to probe addresses
                let mut probe = |addr: u8| -> Option<u8> {
                    let mut who = [0u8];
                    match bus_device.write_read(addr, &[0x00], &mut who) {
                        Ok(()) => {
                            // println!("IMU probe ok addr 0x{:02X} WHO 0x{:02X}", addr, who[0]);
                            Some(who[0])
                        }
                        Err(_e) => {
                            // println!("IMU probe fail addr 0x{:02X}: {:?}", addr, e);
                            None
                        }
                    }
                };

                // First attempt
                let mut found = None;
                for &addr in &[DEFAULT_I2C_ADDR, 0x6A] {
                    if let Some(who) = probe(addr) {
                        found = Some((addr, who));
                        break;
                    }
                }

                // If not found, wait and re-probe (handles power-up race)
                if found.is_none() {
                    for _ in 0..50 {
                        core::hint::spin_loop();
                    }
                    for &addr in &[DEFAULT_I2C_ADDR, 0x6A] {
                        if let Some(who) = probe(addr) {
                            found = Some((addr, who));
                            break;
                        }
                    }
                }

                if let Some((addr, _who)) = found {
                    match Qmi8658::new(bus_device, addr) {
                        Ok(dev) => {
                            // Ok(mut dev) => {
                            // println!("IMU WHO_AM_I (driver): 0x{:02X}", who);
                            // match (dev.read_reg8(0x02), dev.read_reg8(0x09)) {
                            //     (Ok(c1), Ok(c8)) => println!("IMU CTRL1=0x{:02X} CTRL8=0x{:02X}", c1, c8),
                            //     _ => println!("IMU ctrl read failed"),
                            // }
                            Some(dev)
                        }
                        Err(_e) => {
                            // println!("IMU init failed: {:?}", e);
                            None
                        }
                    }
                } else {
                    // println!("IMU not found on scanned addresses");
                    None
                }
            }
            Err(_e) => {
                // println!("I2C init failed: {:?}", e);
                None
            }
        }
    };

    #[cfg(feature = "esp32s3-disp143Oled")]
    let mut smash_detector = SmashDetector::default_rough();
    #[cfg(feature = "esp32s3-disp143Oled")]
    let mut last_sample: Option<esp32s3_tests::qmi8658_imu::ImuSample> = None;
    #[cfg(feature = "esp32s3-disp143Oled")]
    let mut next_poll_ms: u64 = 0;

    // count smash gestures while on Omnitrix page
    #[cfg(feature = "esp32s3-disp143Oled")]
    let mut smash_count: u8 = 0;

    // Debug output of IMU data
    // #[cfg(feature = "esp32s3-disp143Oled")]
    // let mut dbg_next_ms: u64 = 0;
    // #[cfg(feature = "esp32s3-disp143Oled")]
    // let mut _dbg_next_ms: u64 = 0;

    // // -------------------- UI Init --------------------

    #[cfg(feature = "esp32s3-disp143Oled")]
    {
        // Pre-cache Omnitrix logo image
        let _ = precache_asset(AssetId::Logo);
    }

    // Initial UI draw (timed)
    {
        // let t0 = SystemTimer::unit_value(Unit::Unit0);
        update_ui(&mut my_display, last_ui_state, needs_redraw);
        // let t1 = SystemTimer::unit_value(Unit::Unit0);
        // esp_println::println!("Initial UI draw: {} us", to_us(t0, t1));
    }

    needs_redraw = false;

    #[cfg(feature = "esp32s3-disp143Oled")]
    {
        // Pre-cache all Omnitrix images

        use esp32s3_tests::ui::precache_all;
        let _n = precache_all();
        // esp_println::println!("Precached {} Omnitrix images", n);
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

    // Main loop: handle UI, buttons, rotary, and IMU-triggered smash input
    loop {
        let now_ms = {
            let t = SystemTimer::unit_value(Unit::Unit0);
            t.saturating_mul(1000) / SystemTimer::ticks_per_second()
        };

        // Check for UI state changes
        let ui_state = critical_section::with(|cs| UI_STATE.borrow(cs).get());
        if ui_state != last_ui_state {
            last_ui_state = ui_state;
            needs_redraw = true;
        }
        let in_omnitrix = matches!(ui_state.page, Page::Omnitrix(_));
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
            if esp32s3_tests::ui::brightness_take_dirty() {
                needs_redraw = true;
            }
        }

        // Keep redrawing while the Transform dialog is visible so the helix animates.
        if matches!(ui_state.dialog, Some(Dialog::TransformPage)) {
            needs_redraw = true;
        }

        update_ui(&mut my_display, last_ui_state, needs_redraw);
        needs_redraw = false;

        // IMU smash detection
        #[cfg(feature = "esp32s3-disp143Oled")]
        if let Some(dev) = imu.as_mut() {
            // Only read when IMU INT fired, additional fall back to periodic reads if INT never comes.
            let timed = now_ms >= next_poll_ms;
            let pin_level_trig = critical_section::with(|cs| {
                IMU_INT
                    .input
                    .borrow_ref(cs)
                    .as_ref()
                    .map(|p| p.is_low())
                    .unwrap_or(false)
            });
            let should_read = IMU_INT_FLAG.swap(false, Ordering::Relaxed)
                || pin_level_trig
                || last_sample.is_none()
                || timed;
            if should_read {
                // Read sample
                match dev.read_sample() {
                    Ok(sample) => {
                        // Process sample for smash detection
                        if smash_detector.update(now_ms, &sample) {
                            // println!("IMU smash hit:");

                            // the omnitrix page is the only one that uses this input
                            if in_omnitrix {
                                smash_count = smash_count.saturating_add(1);
                                // 2 smashes as it will count both the pop up and the down slam
                                if smash_count >= 1 {
                                    // reset count after triggering
                                    smash_count = 0;
                                    BUTTON3_PRESSED.store(true, Ordering::Relaxed);
                                }
                            }
                        }
                        last_sample = Some(sample);
                    }
                    Err(e) => println!("IMU read failed: {:?}", e),
                }

                if timed {
                    next_poll_ms = now_ms.saturating_add(50);
                }
            }
        }

        // Handle button events
        let b1_event = BUTTON1_PRESSED.swap(false, Ordering::Acquire);
        let b2_event = BUTTON2_PRESSED.swap(false, Ordering::Acquire);

        #[cfg(feature = "esp32s3-disp143Oled")]
        {
            // Track button 1 hold for deep sleep trigger
            let btn1_down = critical_section::with(|cs| {
                BUTTON1
                    .input
                    .borrow_ref(cs)
                    .as_ref()
                    .map(|p| p.is_low())
                    .unwrap_or(false)
            });

            // Start tracking hold when button 1 goes down
            if btn1_down && sleep_hold_start.is_none() {
                sleep_hold_start = Some(now_ms);
            }
            // Reset if button released
            if !btn1_down {
                sleep_hold_start = None;
            }

            // Check for 5-second hold to enter deep sleep
            if let Some(t0) = sleep_hold_start {
                if now_ms.saturating_sub(t0) >= SLEEP_HOLD_MS && btn1_down {
                    // Save clock time to RTC (RTC continues during deep sleep)
                    let current_clock_secs = get_clock_seconds();
                    let rtc_now_us = rtc.current_time_us();
                    let elapsed_since_boot_us = rtc_now_us.saturating_sub(rtc_boot_time_us);
                    let clock_total_us = (current_clock_secs as u64) * 1_000_000
                        + (elapsed_since_boot_us % 1_000_000);
                    rtc.set_current_time_us(clock_total_us);

                    // Disable display
                    let mut delay = TimerDelay;
                    let _ = my_display.disable(&mut delay);

                    // Wait for button 1 release
                    loop {
                        let btn1_released = critical_section::with(|cs| {
                            BUTTON1
                                .input
                                .borrow_ref(cs)
                                .as_ref()
                                .map(|b| b.is_high())
                                .unwrap_or(true)
                        });
                        if btn1_released {
                            break;
                        }
                        delay.delay_ms(10);
                    }
                    delay.delay_ms(50);

                    // Release button pins for reconfiguration
                    critical_section::with(|cs| {
                        let _ = BUTTON1.input.borrow_ref_mut(cs).take();
                        let _ = BUTTON2.input.borrow_ref_mut(cs).take();
                    });

                    // Configure GPIO7 (Button 2) as wake source with RTC pull-up
                    // uses unsafe steal since we've released the pin from earlier
                    let gpio7 = unsafe { esp_hal::peripherals::GPIO7::steal() };
                    use esp_hal::gpio::RtcPinWithResistors;
                    gpio7.rtcio_pullup(true);
                    gpio7.rtcio_pulldown(false);
                    let ext0_wake = Ext0WakeupSource::new(gpio7, WakeupLevel::Low);

                    // Enter deep sleep (resets on wake)
                    rtc.sleep_deep(&[&ext0_wake]);
                }
            }
        }

        // Button 1 = Back (go up a layer)
        if b1_event {
            if esp32s3_tests::ui::watch_edit_active() {
                esp32s3_tests::ui::watch_edit_cancel();
            } else {
                critical_section::with(|cs| {
                    let state = UI_STATE.borrow(cs).get();
                    let new_state = state.back();
                    UI_STATE.borrow(cs).set(new_state);
                });
            }
            needs_redraw = true;
        }

        // Button 2 = Select (enter/confirm)
        if b2_event {
            let ui_state = critical_section::with(|cs| UI_STATE.borrow(cs).get());
            if matches!(
                ui_state.page,
                Page::Watch(esp32s3_tests::ui::WatchAppState::Digital)
            ) {
                if esp32s3_tests::ui::watch_edit_active() {
                    esp32s3_tests::ui::watch_edit_advance();
                } else {
                    esp32s3_tests::ui::watch_edit_start();
                }
            } else {
                critical_section::with(|cs| {
                    let state = UI_STATE.borrow(cs).get();
                    let new_state = state.select();
                    UI_STATE.borrow(cs).set(new_state);
                });
            }
            needs_redraw = true;
        }

        // Button 3 = Transform (IMU will actually trigger this, electrically this will be disconnected)
        if BUTTON3_PRESSED.swap(false, Ordering::Acquire) {
            critical_section::with(|cs| {
                let state = UI_STATE.borrow(cs).get();
                let new_state = state.transform(); // use Omnitrix-only dialog
                UI_STATE.borrow(cs).set(new_state);
            });
            if in_omnitrix {
                needs_redraw = true;
            }
        }

        // Rotary encoder handling
        let pos = critical_section::with(|cs| ROTARY.position.borrow(cs).get());
        let detent = pos / DETENT_STEPS; // use division (works well for negatives too)

        // If detent changed, update UI state
        if Some(detent) != last_detent {
            if let Some(prev) = last_detent {
                let step_delta = detent - prev;
                let ui_state = critical_section::with(|cs| UI_STATE.borrow(cs).get());
                if esp32s3_tests::ui::watch_edit_active() {
                    esp32s3_tests::ui::watch_edit_adjust(-step_delta);
                } else if matches!(
                    ui_state.page,
                    Page::Settings(SettingsMenuState::BrightnessAdjust)
                ) {
                    let new_pct = brightness_adjust(-step_delta);
                    #[cfg(feature = "esp32s3-disp143Oled")]
                    apply_brightness(&mut my_display, new_pct);
                } else if step_delta > 0 {
                    // turned clockwise: go to next state
                    critical_section::with(|cs| {
                        let state = UI_STATE.borrow(cs).get();
                        let new_state = state.prev_item();
                        UI_STATE.borrow(cs).set(new_state);
                    });
                } else if step_delta < 0 {
                    // turned counter-clockwise: go to previous state (optional)
                    critical_section::with(|cs| {
                        let state = UI_STATE.borrow(cs).get();
                        let new_state = state.next_item();
                        UI_STATE.borrow(cs).set(new_state);
                    });
                }
            }
            last_detent = Some(detent);
            needs_redraw = true;
        }

        // If we just exited watch edit, sync external RTC with current software clock.
        #[cfg(feature = "esp32s3-disp143Oled")]
        {
            let edit_active = esp32s3_tests::ui::watch_edit_active();
            if last_watch_edit_active && !edit_active {
                if let Some(bus_ref) = rtc_bus {
                    let dev = embedded_hal_bus::i2c::RefCellDevice::new(bus_ref);
                    let mut rtc_handle = Pcf85063::new(dev);
                    let secs = clock_now_seconds_u32();
                    let dt = unix_to_datetime(secs);
                    let _ = rtc_handle.set_datetime(&dt);
                }
            }
            last_watch_edit_active = edit_active;
        }

        // Minimal delay to keep polling responsive
    }
}
