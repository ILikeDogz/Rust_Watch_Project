//! Display setup and initialization module.
//!
//! This module provides:
//! - The `setup_display` function to configure and initialize the GC9A01 display
//! - The `SpinDelay` struct implementing embedded-hal delay traits for display timing
//!
//! Handles SPI peripheral setup, pin configuration, hardware reset, and display driver initialization.
//! Designed for use with ESP-HAL, mipidsi, and embedded-graphics on 240x240 round LCDs.


use esp_backtrace as _;

// ESP-HAL imports
use esp_hal::{
    gpio::Output,
    spi::master::{Spi, Config as SpiConfig},
    spi::Mode,
    time::Rate,
    Blocking,
    peripherals::{SPI2, GPIO10, GPIO11},
};

// Display interface and device
use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
use mipidsi::interface::SpiInterface;                

// GC9A01 display driver
use mipidsi::{
    Builder as DisplayBuilder,
    models::GC9A01,
    options::{ColorOrder, Orientation, Rotation, ColorInversion},
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

pub fn setup_display<'a>(
    spi2: SPI2<'a>,
    spi_sck: GPIO10<'a>,
    spi_mosi: GPIO11<'a>,
    lcd_cs: Output<'a>,
    lcd_dc: Output<'a>,
    mut lcd_rst: Output<'a>,
    mut lcd_bl: Output<'a>,
    display_buf: &'a mut [u8],
) -> mipidsi::Display<
    SpiInterface<'a, 
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

    // create SPI instance
    let spi = Spi::new(spi2, spi_cfg).unwrap()
        .with_sck(spi_sck)
        .with_mosi(spi_mosi);

    // create display interface
    let spi_device = ExclusiveDevice::new(spi, lcd_cs, NoDelay).unwrap();
    let di = SpiInterface::new(spi_device, lcd_dc, display_buf);
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


