// This module handles board-specific pin mappings and initialization.
// Different profiles can be selected via Cargo features.
// Alternate profiles can be defined for different boards by enabling
// features like "devkit" or "alt" in Cargo.toml.
// OLED is the only one fully supported here, others are wip or templates.

use esp_backtrace as _;

// ESP-HAL imports
use esp_hal::{
    gpio::{Event, Input, InputConfig, Io, Level, Output, OutputConfig, Pull},
    peripherals::{Peripherals, I2C0, SPI2},
};

#[cfg(feature = "devkit-esp32s3-disp128")]
use esp_hal::peripherals::{GPIO10, GPIO11};

#[cfg(feature = "esp32s3-disp143Oled")]
use esp_hal::peripherals::{DMA_CH0, GPIO10, GPIO11, GPIO12, GPIO13, GPIO14, GPIO47, GPIO48};

pub struct BoardPins<'a> {
    // Leds
    // pub led1: Output<'a>,
    // pub led2: Output<'a>,

    // Buttons
    pub btn1: Input<'a>,
    pub btn2: Input<'a>,
    pub btn3: Input<'a>,

    // Rotary encoder pins
    pub enc_clk: Input<'a>,
    pub enc_dt: Input<'a>,

    // IMU interrupt (active-low on GPIO8 per Waveshare schematic)
    #[cfg(feature = "esp32s3-disp143Oled")]
    pub imu_int: Input<'a>,
    // pub enc_sw:  Input<'a>,  // not used in this example

    // display-related pins are feature gated
    #[cfg(any(feature = "devkit-esp32s3-disp128"))]
    pub display_pins: DisplayPins<'a>,
    #[cfg(any(feature = "esp32s3-disp143Oled"))]
    pub display_pins: DisplayPins<'a>,
    // shared I2C bus for touch/IMU
    #[cfg(feature = "esp32s3-disp143Oled")]
    pub imu_i2c: ImuI2cPins<'a>,
}

// nested, feature-only struct for LCD/SPI pins
#[cfg(any(feature = "devkit-esp32s3-disp128"))]
pub struct DisplayPins<'a> {
    // SPI2 pins (SCK, MOSI) are fixed to GPIO10 and GPIO11
    pub spi2: SPI2<'a>,       // SPI2 peripheral
    pub spi_sck: GPIO10<'a>,  // GPIO10 is SPI2 SCK
    pub spi_mosi: GPIO11<'a>, // GPIO11 is SPI
    // LCD control pins
    pub lcd_cs: Output<'a>,  // GPIO9
    pub lcd_dc: Output<'a>,  // GPIO8
    pub lcd_rst: Output<'a>, // GPIO14
    pub lcd_bl: Output<'a>,  // GPIO2
}
#[cfg(any(feature = "esp32s3-disp143Oled"))]
pub struct DisplayPins<'a> {
    // CS=GPIO9, CLK=GPIO10, dO0=GPIO11, dO1=GPIO12, dO2=GPIO13, dO3=GPIO14, RST=GPIO21, EN=GPIO42, TP_SDA=GPIO47, TP_SCL=GPIO48
    pub spi2: SPI2<'a>, // the SPI2 peripheral handle
    pub cs: Output<'a>, // GPIO9
    // pub clk: Output<'a>,
    // pub do0: Output<'a>,
    pub clk: GPIO10<'a>,      // GPIO10
    pub do0: GPIO11<'a>,      // GPIO11
    pub do1: GPIO12<'a>,      // GPIO12
    pub do2: GPIO13<'a>,      // GPIO13
    pub do3: GPIO14<'a>,      // GPIO14
    pub rst: Output<'a>,      // GPIO21
    pub en: Output<'a>,       // GPIO42
    pub dma_ch0: DMA_CH0<'a>, // <- DMA channel for SPI2
}

#[cfg(feature = "esp32s3-disp143Oled")]
pub struct ImuI2cPins<'a> {
    pub sda: GPIO47<'a>,
    pub scl: GPIO48<'a>,
}

// Default profile
#[cfg(feature = "devkit-esp32s3-disp128")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io<'a>, BoardPins<'a>, I2C0<'a>) {
    let io = Io::new(p.IO_MUX);
    let i2c0 = p.I2C0;

    // LEDs
    // let mut led1 = Output::new(p.GPIO1,  Level::Low, OutputConfig::default());
    // let mut led2 = Output::new(p.GPIO19, Level::Low, OutputConfig::default());
    // led1.set_high();
    // led2.set_high();

    // buttons
    let mut btn1 = Input::new(p.GPIO15, InputConfig::default().with_pull(Pull::Up));
    let mut btn2 = Input::new(p.GPIO21, InputConfig::default().with_pull(Pull::Up));
    let mut btn3 = Input::new(p.GPIO45, InputConfig::default().with_pull(Pull::Up));
    btn1.listen(Event::AnyEdge);
    btn2.listen(Event::AnyEdge);
    btn3.listen(Event::AnyEdge);

    // rotary encoder pins
    let mut enc_clk = Input::new(p.GPIO18, InputConfig::default().with_pull(Pull::None));
    let mut enc_dt = Input::new(p.GPIO17, InputConfig::default().with_pull(Pull::None));
    enc_clk.listen(Event::AnyEdge);
    enc_dt.listen(Event::AnyEdge);

    // LCD control pins — do NOT touch GPIO10/11 here (SPI SCK/MOSI)
    let lcd_cs = Output::new(p.GPIO9, Level::High, OutputConfig::default());
    let lcd_dc = Output::new(p.GPIO8, Level::Low, OutputConfig::default());
    let lcd_rst = Output::new(p.GPIO14, Level::High, OutputConfig::default());
    let lcd_bl = Output::new(p.GPIO2, Level::High, OutputConfig::default());

    // SPI2 peripheral and pins
    let spi2 = p.SPI2;
    let spi_sck = p.GPIO10; // GPIO10 is SPI2 SCK
    let spi_mosi = p.GPIO11; // GPIO11 is SPI2 MOSI

    // Return IO handler and all pins
    (
        io,
        BoardPins {
            // led1, led2,
            btn1,
            btn2,
            btn3,
            enc_clk,
            enc_dt,
            display_pins: DisplayPins {
                spi2,
                spi_sck,
                spi_mosi,
                lcd_cs,
                lcd_dc,
                lcd_rst,
                lcd_bl,
            },
        },
        i2c0,
    )
}

// OLED profile
#[cfg(feature = "esp32s3-disp143Oled")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io<'a>, BoardPins<'a>, I2C0<'a>) {
    // use esp_hal::gpio::DriveStrength;

    let io = Io::new(p.IO_MUX);
    let i2c0 = p.I2C0;

    // LEDs
    // let mut led1 = Output::new(p.GPIO1,  Level::Low, OutputConfig::default());
    // let mut led2 = Output::new(p.GPIO19, Level::Low, OutputConfig::default());
    // led1.set_high();
    // led2.set_high();

    // buttons
    let mut btn1 = Input::new(p.GPIO6, InputConfig::default().with_pull(Pull::Up)); //was 45
    let mut btn2 = Input::new(p.GPIO7, InputConfig::default().with_pull(Pull::Up)); //was 46
    let mut btn3 = Input::new(p.GPIO1, InputConfig::default().with_pull(Pull::Up)); //was 1
    btn1.listen(Event::AnyEdge);
    btn2.listen(Event::AnyEdge);
    btn3.listen(Event::AnyEdge);

    // rotary encoder pins
    let mut enc_clk = Input::new(p.GPIO16, InputConfig::default().with_pull(Pull::None)); //was 2
    let mut enc_dt = Input::new(p.GPIO17, InputConfig::default().with_pull(Pull::None)); //was 3
    enc_clk.listen(Event::AnyEdge);
    enc_dt.listen(Event::AnyEdge);

    // OLED control pins
    let cs = Output::new(p.GPIO9, Level::High, OutputConfig::default());
    let rst = Output::new(p.GPIO21, Level::High, OutputConfig::default());
    let en = Output::new(p.GPIO42, Level::Low, OutputConfig::default());

    // SPI2 peripheral and pins
    let spi2 = p.SPI2;
    // let clk = Output::new(
    //     p.GPIO10,
    //     Level::Low,
    //     OutputConfig::default().with_drive_strength(DriveStrength::_20mA),
    // );

    // let do0 = Output::new(
    //     p.GPIO11,
    //     Level::Low,
    //     OutputConfig::default().with_drive_strength(DriveStrength::_20mA),
    // );

    let clk = p.GPIO10; // GPIO10
    let do0 = p.GPIO11; // GPIO11
    let do1 = p.GPIO12; // GPIO12
    let do2 = p.GPIO13; // GPIO13
    let do3 = p.GPIO14; // GPIO14

    // Touch/IMU shared I2C pins (QMI8658 + touch controller sit here on the Waveshare board)
    let imu_sda = p.GPIO47;
    let imu_scl = p.GPIO48;
    let mut imu_int = Input::new(p.GPIO8, InputConfig::default().with_pull(Pull::Up));
    imu_int.listen(Event::AnyEdge);

    // DMA peripheral
    let dma_ch0 = p.DMA_CH0;

    // Return IO handler and all pins
    (
        io,
        BoardPins {
            // led1, led2,
            btn1,
            btn2,
            btn3,
            enc_clk,
            enc_dt,
            imu_int,
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
                dma_ch0,
            },
            imu_i2c: ImuI2cPins {
                sda: imu_sda,
                scl: imu_scl,
            },
        },
        i2c0,
    )
}

// Example alternate profile (enable with --features allinone)
#[cfg(feature = "allinone")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io<'a>, BoardPins<'a>, I2C0<'a>) {
    let io = Io::new(p.IO_MUX);
    let i2c0 = p.I2C0;

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
    let mut enc_dt = Input::new(p.GPIO17, InputConfig::default().with_pull(Pull::None));
    enc_clk.listen(Event::AnyEdge);
    enc_dt.listen(Event::AnyEdge);

    // LCD control pins — do NOT touch GPIO10/11 here (SPI SCK/MOSI)
    let lcd_cs = Output::new(p.GPIO9, Level::High, OutputConfig::default());
    let lcd_dc = Output::new(p.GPIO8, Level::Low, OutputConfig::default());
    let lcd_rst = Output::new(p.GPIO14, Level::High, OutputConfig::default());
    let lcd_bl = Output::new(p.GPIO2, Level::High, OutputConfig::default());

    // SPI2 peripheral and pins
    let spi2 = p.SPI2;
    let spi_sck = p.GPIO10; // GPIO10 is SPI2 SCK
    let spi_mosi = p.GPIO11; // GPIO11 is SPI2 MOSI

    // Return IO handler and all pins
    (
        io,
        BoardPins {
            // led1, led2,
            btn1,
            btn2,
            enc_clk,
            enc_dt,
            spi2,
            spi_sck,
            spi_mosi,
            lcd_cs,
            lcd_dc,
            lcd_rst,
            lcd_bl,
        },
        i2c0,
    )
}

// Yet another profile (enable with --features alt)
#[cfg(feature = "alt")]
pub fn init_board_pins<'a>(p: Peripherals) -> (Io<'a>, BoardPins<'a>, I2C0<'a>) {
    let mut io = Io::new(p.IO_MUX);
    // …map different pins here…
    unimplemented!()
}
