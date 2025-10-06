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
use esp_hal::peripherals::{Peripherals, SPI2, GPIO10, GPIO11};

pub struct BoardPins<'a> {
    pub led1: Output<'a>,
    pub btn1: Input<'a>,
    pub led2: Output<'a>,
    pub btn2: Input<'a>,
    pub enc_clk: Input<'a>,
    pub enc_dt:  Input<'a>,
    // NEW:
    pub lcd_cs:  Output<'a>,  // GPIO9
    pub lcd_dc:  Output<'a>,  // GPIO8
    pub lcd_rst: Output<'a>,  // GPIO14
    pub lcd_bl:  Output<'a>,  // GPIO2
}

// Default profile
#[cfg(feature = "esp32s3")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io<'a>, BoardPins<'a>, SPI2<'a>, GPIO10<'a>, GPIO11<'a>) {
    let io = Io::new(p.IO_MUX);

    // LEDs
    let mut led1 = Output::new(p.GPIO1,  Level::Low, OutputConfig::default());
    let mut led2 = Output::new(p.GPIO19, Level::Low, OutputConfig::default());
    led1.set_high();
    led2.set_high();

    // buttons
    let mut btn1 = Input::new(p.GPIO15, InputConfig::default().with_pull(Pull::Up));
    let mut btn2 = Input::new(p.GPIO21, InputConfig::default().with_pull(Pull::Up));
    btn1.listen(Event::AnyEdge);
    btn2.listen(Event::AnyEdge);

    // rotary encoder pins
    let mut enc_clk = Input::new(p.GPIO18, InputConfig::default().with_pull(Pull::None));
    let mut enc_dt  = Input::new(p.GPIO17, InputConfig::default().with_pull(Pull::None));
    enc_clk.listen(Event::AnyEdge);
    enc_dt.listen(Event::AnyEdge);

    // LCD control pins — do NOT touch GPIO10/11 here (SPI SCK/MOSI)
    let lcd_cs  = Output::new(p.GPIO9,  Level::High, OutputConfig::default());
    let lcd_dc  = Output::new(p.GPIO8,  Level::Low,  OutputConfig::default());
    let lcd_rst = Output::new(p.GPIO14, Level::High, OutputConfig::default());
    let lcd_bl  = Output::new(p.GPIO2,  Level::High, OutputConfig::default());

    // hand SPI2 back before we lose ownership of Peripherals
    let spi2 = p.SPI2;

    (
        io,
        BoardPins {
            led1, btn1, led2, btn2, enc_clk, enc_dt,
            lcd_cs, lcd_dc, lcd_rst, lcd_bl,
        },
        spi2,
        p.GPIO10,
        p.GPIO11,
    )
}

// Example alternate profile (enable with --features devkit)
#[cfg(feature = "devkit")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io<'a>, BoardPins<'a>, SPI2) {
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

    // LCD control pins — same mapping as esp32s3 profile to keep firmware identical
    let lcd_cs  = Output::new(p.GPIO9,  Level::High, OutputConfig::default());
    let lcd_dc  = Output::new(p.GPIO8,  Level::Low,  OutputConfig::default());
    let lcd_rst = Output::new(p.GPIO14, Level::High, OutputConfig::default());
    let lcd_bl  = Output::new(p.GPIO2,  Level::High, OutputConfig::default());

    let spi2 = p.SPI2;

    (
        io,
        BoardPins {
            led1, btn1, led2, btn2, enc_clk, enc_dt,
            lcd_cs, lcd_dc, lcd_rst, lcd_bl,
        },
        spi2,
    )
}


// Yet another profile (enable with --features alt)
#[cfg(feature = "alt")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io, BoardPins<'a>) {
    let mut io = Io::new(p.IO_MUX);
    // …map different pins here…
    unimplemented!()
}