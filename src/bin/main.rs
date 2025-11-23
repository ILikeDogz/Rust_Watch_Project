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

use esp32s3_tests::{
    display::setup_display,
    qmi8658_imu::{Qmi8658, SmashDetector, DEFAULT_I2C_ADDR},
    input::{handle_button_generic, handle_encoder_generic, handle_imu_int_generic, ButtonState, ImuIntState, RotaryState},
    ui::{precache_asset, update_ui, AssetId, MainMenuState, Page, UiState},
    wiring::{init_board_pins, BoardPins},
};

use core::cell::{Cell, RefCell};
use critical_section::Mutex;
use esp_backtrace as _;

// ESP-HAL imports
use esp_hal::{
    handler, i2c::master::{Config as I2cConfig, I2c}, main, psram, ram, time::Rate,
    timer::systimer::{SystemTimer, Unit},
    Config,
};

use esp_println::println;

extern crate alloc;
use alloc::{boxed::Box, vec};

// Embedded-graphics
// use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb565, prelude::RgbColor};

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
    // rotary encoder detent tracking
    const DETENT_STEPS: i32 = 4; // set to 4 if your encoder is 4 steps per detent
    let mut last_detent: Option<i32> = None;

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
        btn1, btn2, btn3,
        enc_clk, enc_dt,
        #[cfg(feature = "esp32s3-disp143Oled")]
        imu_int,
        display_pins,
        #[cfg(feature = "esp32s3-disp143Oled")]
        imu_i2c,
    } = pins;

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

    // -------------------- IMU Init --------------------

    #[cfg(feature = "esp32s3-disp143Oled")]
    let mut imu = {
        let cfg = I2cConfig::default().with_frequency(Rate::from_khz(400));
        match I2c::new(i2c0, cfg) {
            Ok(i2c) => {
                let mut i2c = i2c.with_sda(imu_i2c.sda).with_scl(imu_i2c.scl);

                // Small helper to probe addresses
                let mut probe = |addr: u8| -> Option<u8> {
                    let mut who = [0u8];
                    match i2c.write_read(addr, &[0x00], &mut who) {
                        Ok(()) => {
                            println!("IMU probe ok addr 0x{:02X} WHO 0x{:02X}", addr, who[0]);
                            Some(who[0])
                        }
                        Err(e) => {
                            println!("IMU probe fail addr 0x{:02X}: {:?}", addr, e);
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

                if let Some((addr, who)) = found {
                    match Qmi8658::new(i2c, addr) {
                        Ok(mut dev) => {
                            println!("IMU WHO_AM_I (driver): 0x{:02X}", who);
                            match (dev.read_reg8(0x02), dev.read_reg8(0x09)) {
                                (Ok(c1), Ok(c8)) => println!("IMU CTRL1=0x{:02X} CTRL8=0x{:02X}", c1, c8),
                                _ => println!("IMU ctrl read failed"),
                            }
                            Some(dev)
                        }
                        Err(e) => {
                            println!("IMU init failed: {:?}", e);
                            None
                        }
                    }
                } else {
                    println!("IMU not found on scanned addresses");
                    None
                }
            }
            Err(e) => {
                println!("I2C init failed: {:?}", e);
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
    // // Demo sequence timing
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


    // -------------------- Main loop --------------------
    
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
                                if smash_count >= 2 {
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


        // Debug output of IMU data
        // #[cfg(feature = "esp32s3-disp143Oled")]
        // if now_ms >= dbg_next_ms {
        //     if let Some(s) = last_sample {
        //         let mag_sq = s.accel_mag_sq();
        //         let dot = smash_detector.gravity_dot(&s);
        //         println!(
        //             "DBG a=[{}, {}, {}] |a|^2={} dot={} gyro=[{}, {}, {}] int_flag={} pin_low={}",
        //             s.accel[0],
        //             s.accel[1],
        //             s.accel[2],
        //             mag_sq,
        //             dot,
        //             s.gyro[0],
        //             s.gyro[1],
        //             s.gyro[2],
        //             IMU_INT_FLAG.load(Ordering::Relaxed),
        //             critical_section::with(|cs| {
        //                 IMU_INT
        //                     .input
        //                     .borrow_ref(cs)
        //                     .as_ref()
        //                     .map(|p| p.is_low())
        //                     .unwrap_or(false)
        //             })
        //         );
        //     }
        //     dbg_next_ms = now_ms.saturating_add(200);
        // }

        // Button 1 = Back (go up a layer)
        if BUTTON1_PRESSED.swap(false, Ordering::Acquire) {
            critical_section::with(|cs| {
                let state = UI_STATE.borrow(cs).get();
                let new_state = state.back();
                UI_STATE.borrow(cs).set(new_state);
            });
            needs_redraw = true;
        }

        // Button 2 = Select (enter/confirm)
        if BUTTON2_PRESSED.swap(false, Ordering::Acquire) {
            critical_section::with(|cs| {
                let state = UI_STATE.borrow(cs).get();
                let new_state = state.select();
                UI_STATE.borrow(cs).set(new_state);
            });
            needs_redraw = true;
        }

        // Button 3 = Transform (IMU will actually trigger this, electrically this will be disconnected)
        if BUTTON3_PRESSED.swap(false, Ordering::Acquire) {
            critical_section::with(|cs| {
                let state = UI_STATE.borrow(cs).get();
                let new_state = state.transform(); // use Omnitrix-only dialog
                UI_STATE.borrow(cs).set(new_state);
            });
            needs_redraw = true;
        }

        // Rotary encoder handling
        let pos = critical_section::with(|cs| ROTARY.position.borrow(cs).get());
        let detent = pos / DETENT_STEPS; // use division (works well for negatives too)

        // If detent changed, update UI state
        if Some(detent) != last_detent {
            if let Some(prev) = last_detent {
                let step_delta = detent - prev;
                if step_delta > 0 {
                    // turned clockwise: go to next state
                    critical_section::with(|cs| {
                        // esp_println::println!("Rotary turned clockwise to detent {} pos {}", detent, pos);
                        let state = UI_STATE.borrow(cs).get();
                        let new_state = state.prev_item();
                        UI_STATE.borrow(cs).set(new_state);
                    });
                } else if step_delta < 0 {
                    // turned counter-clockwise: go to previous state (optional)
                    critical_section::with(|cs| {
                        // esp_println::println!("Rotary turned counter-clockwise to detent {} pos {}", detent, pos);
                        let state = UI_STATE.borrow(cs).get();
                        let new_state = state.next_item();
                        UI_STATE.borrow(cs).set(new_state);
                    });
                }
            }
            last_detent = Some(detent);
            needs_redraw = true;
        }

        // Minimal delay to keep polling responsive
    }
}
