// This module handles board-specific pin mappings and initialization.
// Different profiles can be selected via Cargo features.
// Default profile uses GPIO1 for LED1, GPIO15 for Button1, etc.
// Alternate profiles can be defined for different boards by enabling
// features like "devkit" or "alt" in Cargo.toml.
//! The following wiring is assumed:
//! - LED => GPIO1
//! - BUTTON => GPIO15
//! - LED2 => GPIO19
//! - BUTTON2 => GPIO21
//! - Rotary encoder CLK => GPIO18
//! - Rotary encoder DT  => GPIO17
//! - Rotary encoder SW  => GPIO16 (not used in this example)
//! - GND => GND
//! - 3.3V => 3.3V
//! Make sure the button is connected to GND when pressed (it has a pull-up).
//! The rotary encoder should have no internal pull-ups using external 10k resistors, 
//! and be connected to GND on the other side

use esp_backtrace as _;
use esp_hal::{
    gpio::{Event, Input, InputConfig, Io, Level, Output, OutputConfig, Pull},
};
use esp_hal::peripherals::Peripherals;

pub struct BoardPins<'a> {
    pub led1: Output<'a>,
    pub btn1: Input<'a>,
    pub led2: Output<'a>,
    pub btn2: Input<'a>,
    pub enc_clk: Input<'a>,
    pub enc_dt:  Input<'a>,
}

// Default profile
#[cfg(feature = "esp32s3")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io<'a>, BoardPins<'a>) {
    let io = Io::new(p.IO_MUX);

    let mut led1 = Output::new(p.GPIO1,  Level::Low, OutputConfig::default());
    let mut led2 = Output::new(p.GPIO19, Level::Low, OutputConfig::default());
    led1.set_high();
    led2.set_high();

    let mut btn1 = Input::new(p.GPIO15, InputConfig::default().with_pull(Pull::Up));
    let mut btn2 = Input::new(p.GPIO21, InputConfig::default().with_pull(Pull::Up));
    btn1.listen(Event::AnyEdge);
    btn2.listen(Event::AnyEdge);

    let mut enc_clk = Input::new(p.GPIO18, InputConfig::default().with_pull(Pull::None));
    let mut enc_dt  = Input::new(p.GPIO17, InputConfig::default().with_pull(Pull::None));
    enc_clk.listen(Event::AnyEdge);
    enc_dt.listen(Event::AnyEdge);

    (io, BoardPins { led1, btn1, led2, btn2, enc_clk, enc_dt })
}

// Example alternate profile (enable with --features devkit)
#[cfg(feature = "devkit")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io<'a>, BoardPins<'a>) {
    let io = Io::new(p.IO_MUX);

    let mut led1 = Output::new(p.GPIO4,  Level::Low, OutputConfig::default());
    let mut led2 = Output::new(p.GPIO5,  Level::Low, OutputConfig::default());
    led1.set_high();
    led2.set_high();

    let mut btn1 = Input::new(p.GPIO0,  InputConfig::default().with_pull(Pull::Up));
    let mut btn2 = Input::new(p.GPIO1,  InputConfig::default().with_pull(Pull::Up));
    btn1.listen(Event::AnyEdge);
    btn2.listen(Event::AnyEdge);

    let mut enc_clk = Input::new(p.GPIO6, InputConfig::default().with_pull(Pull::None));
    let mut enc_dt  = Input::new(p.GPIO7, InputConfig::default().with_pull(Pull::None));
    enc_clk.listen(Event::AnyEdge);
    enc_dt.listen(Event::AnyEdge);

    (io, BoardPins { led1, btn1, led2, btn2, enc_clk, enc_dt })
}

// Yet another profile (enable with --features alt)
#[cfg(feature = "alt")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io, BoardPins<'a>) {
    let mut io = Io::new(p.IO_MUX);
    // …map different pins here…
    unimplemented!()
}