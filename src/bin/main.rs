//! GPIO interrupt
//!
//! This prints "Interrupt" when the boot button is pressed.
//! It also toggles an LCD when either button is pressed.
//! It also reads a rotary encoder and prints direction and detent count.

//% CHIPS: esp32s3
//% FEATURES: esp-hal/unstable

#![no_std]
#![no_main]

// Define the application description, which is placed in a special section of the binary. 
// This is used by the bootloader to verify the application. 
// The macro automatically fills in the fields. 
esp_bootloader_esp_idf::esp_app_desc!();

use esp32s3_tests::wiring::init_board_pins;
use esp32s3_tests::wiring::BoardPins;

use esp_backtrace as _;
use core::cell::{Cell, RefCell};
use critical_section::Mutex;

// ESP-HAL imports
use esp_hal::{
    gpio::{Input, Output},
    handler,
    main,
    ram,
    spi::master::{Spi, Config as SpiConfig},
    spi::Mode,
    time::Rate,
    timer::systimer::{SystemTimer, Unit},
    Blocking,
    peripherals::{SPI2, GPIO10, GPIO11},
};

// Display interface and device
use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
use display_interface_spi::SPIInterface;

// GC9A01 display driver
use mipidsi::{
    Builder as DisplayBuilder,
    models::GC9A01,
    options::{ColorOrder, Orientation, Rotation, ColorInversion},
};


// Embedded-graphics
use embedded_graphics::{
    mono_font::{ascii::{FONT_10X20, FONT_6X10}, 
    MonoTextStyle, MonoTextStyleBuilder}, 
    pixelcolor::Rgb565, 
    prelude::{Point, Primitive, RgbColor, Size}, 
    primitives::{PrimitiveStyle, Rectangle, Circle, Triangle}, text::{Alignment, Baseline, Text}, 
    Drawable,
    draw_target::DrawTarget, 
};



struct SpinDelay;

// Implement embedded_hal delay traits for SpinDelay
impl embedded_hal::delay::DelayNs for SpinDelay {
    #[inline]
    fn delay_ns(&mut self, ns: u32) {
        // very rough busy-wait; good enough for init pulses
        // (the driver mostly calls us with µs/ms delays)
        let mut n = ns / 50 + 1;
        while n != 0 { core::hint::spin_loop(); n -= 1; }
    }

    #[inline]
    fn delay_us(&mut self, us: u32) {
        for _ in 0..us { self.delay_ns(1_000); }
    }

    #[inline]
    fn delay_ms(&mut self, ms: u32) {
        for _ in 0..ms { self.delay_us(1_000); }
    }
}


// Shared resources for button handling
struct ButtonState<'a> {
    input: Mutex<RefCell<Option<Input<'a>>>>,
    // led: Mutex<RefCell<Option<Output<'a>>>>,
    last_level: Mutex<Cell<bool>>,
    last_interrupt: Mutex<Cell<u64>>,
    name: &'static str,
}

// Shared resources for rotary encoder handling
struct RotaryState<'a> {
    clk: Mutex<RefCell<Option<Input<'a>>>>,
    dt:  Mutex<RefCell<Option<Input<'a>>>>,
    position:    Mutex<Cell<i32>>,
    last_qstate: Mutex<Cell<u8>>,  // bits: [CLK<<1 | DT]
    last_step: Mutex<Cell<i8>>, // +1 or -1 from last transition
}

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

// Shared resources for rotary encoder
static ROTARY: RotaryState<'static> = RotaryState {
    clk: Mutex::new(RefCell::new(None)),
    dt:  Mutex::new(RefCell::new(None)),
    position:    Mutex::new(Cell::new(0)),
    last_qstate: Mutex::new(Cell::new(0)), // bits: [CLK<<1 | DT]
    last_step: Mutex::new(Cell::new(0)), // +1 or -1 from last transition
};

// UI state variable (example usage)
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum UiState {
    State1,
    State2,
    State3,
    State4,
}

impl UiState {
    // All possible states
    const ALL: [UiState; 4] = [UiState::State1, UiState::State2, UiState::State3, UiState::State4];

    // Cycle to next state
    fn next(self) -> Self {
        use UiState::*;
        match self {
            State1 => State2,
            State2 => State3,
            State3 => State4,
            State4 => State1,
        }
    }

    fn prev(self) -> Self {
        use UiState::*;
        match self {
            State1 => State4,
            State2 => State1,
            State3 => State2,
            State4 => State3,
        }
    }

    // For potential future use: convert to/from u8 for storage
    fn as_u8(self) -> u8 {
        match self {
            UiState::State1 => 1,
            UiState::State2 => 2,
            UiState::State3 => 3,
            UiState::State4 => 4,
        }
    }

    // For potential future use: convert to/from u8 for storage
    fn from_u8(n: u8) -> Self {
        match n {
            1 => UiState::State1,
            2 => UiState::State2,
            3 => UiState::State3,
            4 => UiState::State4,
            _ => UiState::State1,
        }
    }
}

static UI_STATE: Mutex<Cell<UiState>> = Mutex::new(Cell::new(UiState::State1));

// Current debounce time (milliseconds)
const DEBOUNCE_MS: u64 = 240;

// Display configuration, (0,0) is top-left corner
const RESOLUTION: u32 = 240; // 240x240 display
const CENTER: i32 = RESOLUTION as i32 / 2;


// helper function to update the display based on UI_STATE
fn update_ui(
    disp: &mut mipidsi::Display<
        SPIInterface<
            ExclusiveDevice<Spi<'_, Blocking>, Output<'_>, NoDelay>,
            Output<'_>,
        >,
        GC9A01,
        Output<'_>,
    >,
) {
    // Clear display background
    disp.clear(Rgb565::BLACK).ok();

    // Get current state
    let state = critical_section::with(|cs| UI_STATE.borrow(cs).get());

    match state {
        UiState::State1 => {
            // Draw centered text
            let style_bg = MonoTextStyleBuilder::new()
                .font(&FONT_10X20)
                .text_color(Rgb565::WHITE)
                .background_color(Rgb565::GREEN)
                .build();

            Text::with_alignment(
                "State 1",
                Point::new(CENTER, CENTER),
                style_bg,
                Alignment::Center,
            )
            .draw(disp)
            .ok();
        }
        UiState::State2 => {
            // Draw a centered circle
            let diameter: u32 = 120;
            Circle::new(
                Point::new(CENTER - diameter as i32 / 2, CENTER - diameter as i32 / 2),
                diameter,
            )
            .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 5))
            .draw(disp)
            .ok();
        }
        UiState::State3 => {
            // Draw a filled centered rectangle
            let size = Size::new(120, 80);
            Rectangle::new(
                Point::new(CENTER - (size.width as i32 / 2), CENTER - (size.height as i32 / 2)),
                size,
            )
            .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
            .draw(disp)
            .ok();
        }
        UiState::State4 => {
            // Draw a filled centered circle
            let diameter: u32 = 120;
            Circle::new(
                Point::new(CENTER - diameter as i32 / 2, CENTER - diameter as i32 / 2),
                diameter,
            )
            .into_styled(PrimitiveStyle::with_fill(Rgb565::WHITE))
            .draw(disp)
            .ok();
        }
        // Rectangle::new(Point::new(0, 0), Size::new(240, 240))
        //     .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        //     .draw(&mut my_display)
        //     .ok();

        // my_display.fill_solid(
        //     &Rectangle::new(Point::new(0, 0), Size::new(240, 240)),
        //     Rgb565::BLACK
        // ).ok();

        // Rectangle::new(Point::new(0, 0), Size::new(120, 120))
        //     .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
        //     .draw(&mut my_display).ok();

        // Rectangle::new(Point::new(120, 120), Size::new(120, 120))
        //     .into_styled(PrimitiveStyle::with_fill(Rgb565::GREEN))
        //     .draw(&mut my_display).ok();

        // // Centered circle with diameter 160
        // let diameter: u32 = 160;
        // Circle::new(Point::new(CENTER - diameter as i32 / 2, CENTER - diameter as i32 / 2), diameter)
        //     .into_styled(PrimitiveStyle::with_stroke(Rgb565::WHITE, 3))
        //     .draw(&mut my_display)
        //     .ok();
    }
}


fn setup_display<'a>(
    spi2: SPI2<'a>,
    spi_sck: GPIO10<'a>,
    spi_mosi: GPIO11<'a>,
    lcd_cs: Output<'a>,
    lcd_dc: Output<'a>,
    mut lcd_rst: Output<'a>,
    mut lcd_bl: Output<'a>,
) -> mipidsi::Display<
    SPIInterface<
        ExclusiveDevice<Spi<'a, Blocking>, Output<'a>, NoDelay>,
        Output<'a>,
    >,
    GC9A01,
    Output<'a>,>
 {
    // Hardware reset
    lcd_rst.set_low();
    for _ in 0..10000 { core::hint::spin_loop(); }
    lcd_rst.set_high();
    lcd_bl.set_high();

    // SPI setup
    let spi_cfg = SpiConfig::default()
        .with_frequency(Rate::from_hz(40_000_000))
        .with_mode(Mode::_0);

    let spi = Spi::new(spi2, spi_cfg).unwrap()
        .with_sck(spi_sck)
        .with_mosi(spi_mosi);

    let spi_device = ExclusiveDevice::new(spi, lcd_cs, NoDelay).unwrap();
    let di = SPIInterface::new(spi_device, lcd_dc);
    let mut delay = SpinDelay;

    // display set up
    let disp = DisplayBuilder::new(GC9A01, di)
        .display_size(240, 240)
        .display_offset(0, 0)
        .orientation(Orientation::new().rotate(Rotation::Deg180))
        .invert_colors(ColorInversion::Inverted)
        .color_order(ColorOrder::Bgr)
        .reset_pin(lcd_rst) 
        .init(&mut delay) 
        .unwrap();
    
    disp
}





// Handle button press events
fn handle_button_generic(btn: &ButtonState, now_ms: u64) {
    // Access button state within critical section
    critical_section::with(|cs| {
        let mut btn_binding = btn.input.borrow_ref_mut(cs);
        let input = btn_binding.as_mut().unwrap();

        // Check if interrupt is actually pending
        if !input.is_interrupt_set() { 
            return; 
        }
        input.clear_interrupt();

        // Debounce logic: check for falling edge and time since last event
        let level_is_low = input.is_low();
        let last_high = btn.last_level.borrow(cs).get();
        btn.last_level.borrow(cs).set(!level_is_low);

        if last_high && level_is_low {
            // Falling edge detected
            let last_debounce = btn.last_interrupt.borrow(cs).get();
            // Check debounce time
            if now_ms.saturating_sub(last_debounce) > DEBOUNCE_MS {
                // Valid press event
                btn.last_interrupt.borrow(cs).set(now_ms);
                esp_println::println!("{} pressed", btn.name);
                // Toggle associated LED if available
                // if let Some(led) = btn.led.borrow_ref_mut(cs).as_mut() { 
                //     led.toggle(); 
                //     esp_println::println!("{} pressed, LED is now {}", btn.name, if led.is_set_high() { "ON" } else { "OFF" });
                // }

                // Example: update UI state variable
                let state = UI_STATE.borrow(cs).get();
                let new_state = state.next();
                UI_STATE.borrow(cs).set(new_state);
            }
        }
    });
}

#[inline(always)]
fn handle_encoder_generic(encoder: &RotaryState) {
    // Access encoder state within critical section
    critical_section::with(|cs| {
        let mut clk_binding = encoder.clk.borrow_ref_mut(cs);
        let mut dt_binding  = encoder.dt.borrow_ref_mut(cs);
        let clk = clk_binding.as_mut().unwrap();
        let dt  = dt_binding.as_mut().unwrap();

        // Check if interrupt is actually pending on either pin
        if !clk.is_interrupt_set() && !dt.is_interrupt_set() { 
            return; 
        }

        // Clear interrupt flags
        let clk_pending = clk.is_interrupt_set();
        let dt_pending  = dt.is_interrupt_set();
        if clk_pending { clk.clear_interrupt(); }
        if dt_pending  { dt.clear_interrupt(); }

        // Read current state of both pins
        let current = ((clk.is_high() as u8) << 1) | (dt.is_high() as u8);
        let previous = ROTARY.last_qstate.borrow(cs).get();

        // Correct quadrature table for index = (prev<<2)|curr
        // curr order: 00, 01, 10, 11 ; prev blocks: 00, 01, 10, 11
        const TRANS: [i8; 16] = [
            // prev=00: 00, 01, 10, 11
            0, -1, 1,  0,
            // prev=01: 00, 01, 10, 11
            1,  0,  0, -1,
            // prev=10: 00, 01, 10, 11
            -1,  0,  0, 1,
            // prev=11: 00, 01, 10, 11
            0, 1, -1,  0,
        ];

        // Determine step delta from transition table
        let step_delta = TRANS[((previous << 2) | current) as usize];

        // Update position if there was a step
        if step_delta != 0 {
            let p = ROTARY.position.borrow(cs).get().saturating_add(step_delta as i32);
            ROTARY.position.borrow(cs).set(p);
            ROTARY.last_step.borrow(cs).set(step_delta);
        }
        // Save current state for next transition
        ROTARY.last_qstate.borrow(cs).set(current);
    });
}



// Interrupt handler
#[handler]
#[ram]
fn handler() {
    // Get current time in ms
    let now_ms = {
        let t = SystemTimer::unit_value(Unit::Unit0);
        t.saturating_mul(1000) / SystemTimer::ticks_per_second()
    };
    
    handle_button_generic(&BUTTON1, now_ms);
    handle_button_generic(&BUTTON2, now_ms);
    handle_encoder_generic(&ROTARY);
}


#[main]
fn main() -> ! {

    // rotary encoder detent tracking
    const DETENT_STEPS: i32 = 4; // set to 4 if your encoder is 4 steps per detent
    let mut last_detent: Option<i32> = None;

    // initial UI state
    let mut last_ui_state = UiState::State1;

    // Initialize peripherals
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // one call gives you IO handler + all your role pins from wiring.rs
    let (mut io, pins) = init_board_pins(peripherals);
    io.set_interrupt_handler(handler);

    // Destructure pins for easier access
    let BoardPins {
        btn1, btn2,
        enc_clk, enc_dt,
        spi2, spi_sck, spi_mosi,
        lcd_cs, lcd_dc, lcd_rst, lcd_bl,
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

        ROTARY.clk.borrow_ref_mut(cs).replace(enc_clk);
        ROTARY.dt.borrow_ref_mut(cs).replace(enc_dt);
        ROTARY.last_qstate.borrow(cs).set(qstate_initial);
        ROTARY.position.borrow(cs).set(0);
        ROTARY.last_step.borrow(cs).set(0);
    });

    // set up display
    let mut my_display = setup_display(
        spi2, spi_sck, spi_mosi, lcd_cs, lcd_dc, lcd_rst, lcd_bl
    );

    // --- FIRST DRAW ----------------------------------------------------------
    // Clear display by drawing a filled rectangle    
    // Full black background:
    my_display.clear(Rgb565::BLACK).ok();
    update_ui(&mut my_display);

    loop {

        // Check for UI state changes
        let ui_state = critical_section::with(|cs| UI_STATE.borrow(cs).get());
        if ui_state != last_ui_state {
            update_ui(&mut my_display);
            last_ui_state = ui_state;
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
                        let state = UI_STATE.borrow(cs).get();
                        let new_state = state.next();
                        UI_STATE.borrow(cs).set(new_state);
                    });
                } else if step_delta < 0 {
                    // turned counter-clockwise: go to previous state (optional)
                    critical_section::with(|cs| {
                        let state = UI_STATE.borrow(cs).get();
                        let new_state = state.prev();
                        UI_STATE.borrow(cs).set(new_state);
                    });
                }
            }
            last_detent = Some(detent);
        }

        // Small delay to reduce CPU usage
        for _ in 0..10000 { core::hint::spin_loop(); }
    }
}
