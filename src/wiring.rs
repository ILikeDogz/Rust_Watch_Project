// This module handles board-specific pin mappings and initialization.
// Different profiles can be selected via Cargo features.
// Default profile uses GPIO1 for LED1, GPIO15 for Button1, etc.
// Alternate profiles can be defined for different boards by enabling
// features like "devkit" or "alt" in Cargo.toml.
//! The following default wiring is assumed:
//! - LED => GPIO1
//! - LED2 => GPIO19
//! - BUTTON => GPIO15
//! - BUTTON2 => GPIO21
//! - Rotary encoder CLK => GPIO18
//! - Rotary encoder DT  => GPIO17
//! - Rotary encoder SW  => GPIO16 (not used in this example)
//! - GND => GND
//! - 3.3V => 3.3V
//! - SPI2 SCK => GPIO10 (hardware SPI clock)
//! - SPI2 MOSI => GPIO11 (hardware SPI MOSI)
//! - LCD CS  => GPIO9
//! - LCD DC  => GPIO8
//! - LCD RST => GPIO14
//! - LCD BL  => GPIO2
//! Make sure the button is connected to GND when pressed (it has a pull-up).
//! The rotary encoder should have no internal pull-ups using external 10k resistors, 
//! and be connected to GND on the other side
//! (we use interrupt on both edges to detect rotation).
//! The LCD pins can be connected directly to the ESP32-S3 GPIOs
//! as they are 3.3V logic level compatible.
//! Ground and 3.3V pins are available on the board.
//! The SPI2 pins (SCK, MOSI) are fixed and cannot be changed.
//! SCK is GPIO10 and MOSI is GPIO11 on the ESP32-S3.
//! The MISO pin is not used in this example but could be mapped to GPIO12 if needed.


use esp_backtrace as _;

// ESP-HAL imports
use esp_hal::{
    gpio::{Event, Input, InputConfig, Io, Level, Output, OutputConfig, Pull},
    peripherals::{Peripherals, SPI2},
};

#[cfg(feature = "devkit-esp32s3-disp128")]
use esp_hal::{
    peripherals::{GPIO10, GPIO1},
};



#[cfg(feature = "esp32s3-disp143Oled")]
use esp_hal::{
    peripherals::{GPIO10, GPIO11, GPIO12, GPIO13, GPIO14, GPIO47, GPIO48},
};


pub struct BoardPins<'a> {
    // Leds
    // pub led1: Output<'a>,
    // pub led2: Output<'a>,

    // Buttons
    pub btn1: Input<'a>,
    pub btn2: Input<'a>,

    // Rotary encoder pins
    pub enc_clk: Input<'a>,
    pub enc_dt:  Input<'a>,
    // pub enc_sw:  Input<'a>,  // not used in this example

     // display-related pins are feature gated
    #[cfg(any(feature = "devkit-esp32s3-disp128"))]
    pub display_pins: DisplayPins<'a>,
    #[cfg(any(feature = "esp32s3-disp143Oled"))]
    pub display_pins: DisplayPins<'a>,
}

// nested, feature-only struct for LCD/SPI pins
#[cfg(any(feature = "devkit-esp32s3-disp128"))]
pub struct DisplayPins<'a> {
    // SPI2 pins (SCK, MOSI) are fixed to GPIO10 and GPIO11
    pub spi2: SPI2<'a>, // SPI2 peripheral
    pub spi_sck: GPIO10<'a>, // GPIO10 is SPI2 SCK
    pub spi_mosi: GPIO11<'a>,// GPIO11 is SPI
    // LCD control pins
    pub lcd_cs:  Output<'a>,  // GPIO9
    pub lcd_dc:  Output<'a>,  // GPIO8
    pub lcd_rst: Output<'a>,  // GPIO14
    pub lcd_bl:  Output<'a>,  // GPIO2
}
#[cfg(any(feature = "esp32s3-disp143Oled"))]
pub struct DisplayPins<'a> {
    // CS=GPIO9, CLK=GPIO10, dO0=GPIO11, dO1=GPIO12, dO2=GPIO13, dO3=GPIO14, RST=GPIO21, EN=GPIO42, TP_SDA=GPIO47, TP_SCL=GPIO48
    pub spi2: SPI2<'a>,     // <-- new: the SPI2 peripheral handle
    pub cs:  Output<'a>,    // GPIO9
    pub clk: GPIO10<'a>, // Change from Output<'a> to GPIO10<'a>
    pub do0: GPIO11<'a>, // Change from Output<'a> to GPIO11<'a>
    pub do1: GPIO12<'a>,     // GPIO12 if you plan reads later
    pub do2: GPIO13<'a>,    // (unused here)
    pub do3: GPIO14<'a>,    // (unused here)
    pub rst: Output<'a>,    // GPIO21
    pub en:  Output<'a>,    // GPIO42
    pub tp_sda: GPIO47<'a>, // (unused here)
    pub tp_scl: GPIO48<'a>, // (unused here)
}

// Default profile
#[cfg(feature = "devkit-esp32s3-disp128")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io<'a>, BoardPins<'a>) {
    let io = Io::new(p.IO_MUX);

    // LEDs
    // let mut led1 = Output::new(p.GPIO1,  Level::Low, OutputConfig::default());
    // let mut led2 = Output::new(p.GPIO19, Level::Low, OutputConfig::default());
    // led1.set_high();
    // led2.set_high();

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

    // SPI2 peripheral and pins
    let spi2 = p.SPI2; 
    let spi_sck = p.GPIO10; // GPIO10 is SPI2 SCK
    let spi_mosi = p.GPIO11;// GPIO11 is SPI2 MOSI

    // Return IO handler and all pins
    (
        io,
        BoardPins {
            // led1, led2, 
            btn1, btn2,
            enc_clk, enc_dt,
            display_pins: DisplayPins {
                spi2,
                spi_sck,
                spi_mosi,
                lcd_cs, lcd_dc, lcd_rst, lcd_bl,
            },
        },
    )
}


// OLED profile
#[cfg(feature = "esp32s3-disp143Oled")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io<'a>, BoardPins<'a>) {
    let io = Io::new(p.IO_MUX);

    // LEDs
    // let mut led1 = Output::new(p.GPIO1,  Level::Low, OutputConfig::default());
    // let mut led2 = Output::new(p.GPIO19, Level::Low, OutputConfig::default());
    // led1.set_high();
    // led2.set_high();

    // buttons
    let mut btn1 = Input::new(p.GPIO45, InputConfig::default().with_pull(Pull::Up));
    let mut btn2 = Input::new(p.GPIO41, InputConfig::default().with_pull(Pull::Up));
    btn1.listen(Event::AnyEdge);
    btn2.listen(Event::AnyEdge);

    // // rotary encoder pins
    let mut enc_clk = Input::new(p.GPIO18, InputConfig::default().with_pull(Pull::None));
    let mut enc_dt  = Input::new(p.GPIO17, InputConfig::default().with_pull(Pull::None));
    enc_clk.listen(Event::AnyEdge);
    enc_dt.listen(Event::AnyEdge);
    // Enable pull-ups; only CLK generates interrupts (DT sampled in ISR)
    // let mut enc_clk = Input::new(p.GPIO18, InputConfig::default().with_pull(Pull::Up));
    // let mut enc_dt  = Input::new(p.GPIO17, InputConfig::default().with_pull(Pull::Up));
    // enc_clk.listen(Event::AnyEdge);

    // OLED control pins — do NOT touch GPIO10/11 here (SPI SCK/MOSI)
    let cs  = Output::new(p.GPIO9,  Level::High, OutputConfig::default());
    let rst = Output::new(p.GPIO21, Level::High, OutputConfig::default());
    let en  = Output::new(p.GPIO42,  Level::Low, OutputConfig::default());

    // SPI2 peripheral and pins
    let spi2 = p.SPI2;
    let clk = p.GPIO10; // GPIO10 as Output (not SPI)
    let do0 = p.GPIO11; // GPIO11 as Output (MOSI as GPIO)
    // do1 if needed:
    let do1 = p.GPIO12; // GPIO12 as Input (MISO as GPIO)

    let do2 = p.GPIO13; // GPIO13 
    let do3 = p.GPIO14; // GPIO14 

    // Touch controller pins
    let tp_sda = p.GPIO47;
    let tp_scl = p.GPIO48;

    // Return IO handler and all pins
    (
        io,
        BoardPins {
            // led1, led2, 
            btn1, btn2,
            enc_clk, enc_dt,
            display_pins: DisplayPins {
                spi2,
                cs,
                clk,
                do0,
                do1,
                do2,
                do3,
                rst,
                en,
                tp_sda,
                tp_scl,
            },
        },
    )
}


// Example alternate profile (enable with --features allinone)
#[cfg(feature = "allinone")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io<'a>, BoardPins<'a>) {
    let io = Io::new(p.IO_MUX);

    // LEDs
    // let mut led1 = Output::new(p.GPIO1,  Level::Low, OutputConfig::default());
    // let mut led2 = Output::new(p.GPIO19, Level::Low, OutputConfig::default());
    // led1.set_high();
    // led2.set_high();

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

    // SPI2 peripheral and pins
    let spi2 = p.SPI2; 
    let spi_sck = p.GPIO10; // GPIO10 is SPI2 SCK
    let spi_mosi = p.GPIO11;// GPIO11 is SPI2 MOSI

    // Return IO handler and all pins
    (
        io,
        BoardPins {
            // led1, led2, 
            btn1,btn2, 
            enc_clk, enc_dt,
            spi2,
            spi_sck,
            spi_mosi,
            lcd_cs, lcd_dc, lcd_rst, lcd_bl,
        },
    )
}


// Yet another profile (enable with --features alt)
#[cfg(feature = "alt")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io, BoardPins<'a>) {
    let mut io = Io::new(p.IO_MUX);
    // …map different pins here…
    unimplemented!()
}