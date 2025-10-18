//! Display setup and initialization module.
//
// - `setup_display` picks the right backend based on features.
// - Reuses your SpinDelay and DisplayPins wiring.
// - GC9A01 path uses mipidsi (240x240, D/C).
// - CO5300 path uses your no_std driver (466x466, no D/C, 0x02 framing).

use esp_backtrace as _;

// ------------------------- Common imports -------------------------
use esp_hal::{
    gpio::Output,
    spi::master::{Spi, Config as SpiConfig},
    spi::Mode,
    time::Rate,
    Blocking,
};

use embedded_hal::delay::DelayNs;
use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};

use crate::wiring::DisplayPins;
use crate::co5300::{Co5300Display, CO5300_WIDTH, CO5300_HEIGHT};

// A tiny busy-wait delay that satisfies embedded-hal 1.0 DelayNs.
struct SpinDelay;
impl embedded_hal::delay::DelayNs for SpinDelay {
    #[inline]
    fn delay_ns(&mut self, ns: u32) {
        let mut n = ns / 50 + 1;
        while n != 0 { core::hint::spin_loop(); n -= 1; }
    }
    #[inline]
    fn delay_us(&mut self, us: u32) { for _ in 0..us { self.delay_ns(1_000); } }
    #[inline]
    fn delay_ms(&mut self, ms: u32) { for _ in 0..ms { self.delay_us(1_000); } }
}

// ==================================================================
// GC9A01 (240x240) backend  — feature: devkit-esp32s3-disp128
// ==================================================================
#[cfg(feature = "devkit-esp32s3-disp128")]
mod gc9a01_backend {
    use super::*;
    use mipidsi::interface::SpiInterface;
    use mipidsi::{
        Builder as DisplayBuilder,
        models::GC9A01,
        options::{ColorOrder, Orientation, Rotation, ColorInversion},
    };

    pub type DisplayType<'a> = mipidsi::Display<
        SpiInterface<'a,
            ExclusiveDevice<Spi<'a, Blocking>, Output<'a>, NoDelay>,
            Output<'a>,
        >,
        GC9A01,
        Output<'a>,
    >;

    pub fn setup_display<'a>(
        display_pins: DisplayPins<'a>,
        display_buf: &'a mut [u8],
    ) -> DisplayType<'a>
    {
        // Destructure pins
        let DisplayPins {
            spi2,
            spi_sck,
            spi_mosi,
            lcd_cs,
            lcd_dc,
            mut lcd_rst,
            mut lcd_bl,
        } = display_pins;

        // Hardware reset & backlight
        lcd_rst.set_low();
        for _ in 0..10000 { core::hint::spin_loop(); }
        lcd_rst.set_high();
        lcd_bl.set_high();

        // SPI @ 40 MHz, Mode 0
        let spi_cfg = SpiConfig::default()
            .with_frequency(Rate::from_hz(40_000_000))
            .with_mode(Mode::_0);

        let spi = Spi::new(spi2, spi_cfg).unwrap()
            .with_sck(spi_sck)
            .with_mosi(spi_mosi);

        // SPI device + DisplayInterface (needs D/C and a buffer)
        let spi_dev = ExclusiveDevice::new(spi, lcd_cs, NoDelay).unwrap();
        let di = SpiInterface::new(spi_dev, lcd_dc, display_buf);
        let mut delay = SpinDelay;

        // Build GC9A01
        DisplayBuilder::new(GC9A01, di)
            .display_size(240, 240)
            .display_offset(0, 0)
            .orientation(Orientation::new().rotate(Rotation::Deg180))
            .invert_colors(ColorInversion::Inverted)
            .color_order(ColorOrder::Bgr)
            .reset_pin(lcd_rst)
            .init(&mut delay)
            .unwrap()
    }
}

// ==================================================================
// CO5300 (466x466) backend — feature: esp32s3-disp143Oled
// ==================================================================
#[cfg(feature = "esp32s3-disp143Old")]
mod co5300_backend {
    use super::*;

    // Your no_std CO5300 driver
    // use crate::co5300::{Co5300Display, CO5300_WIDTH, CO5300_HEIGHT};

    // Concrete type we return for this backend:
    pub type DisplayType<'a> =
        Co5300Display<ExclusiveDevice<Spi<'a, Blocking>, Output<'a>, NoDelay>, Output<'a>>;

    /// OLED setup:
    /// - `spi2`: the SPI2 peripheral (pass `peripherals.SPI2` from main)
    /// - `display_pins`: your new QSPI-style pin bundle
    /// - `_display_buf`: unused for CO5300 (kept for API parity with TFT path)
    pub fn setup_display<'a>(
        display_pins: DisplayPins<'a>,
        _display_buf: &'a mut [u8],
    ) -> DisplayType<'a> {
        // Destructure the pins as defined in your new wiring
        let DisplayPins {
            // spi2,
            cs,
            clk,
            do0,   // used as MOSI in Standard SPI mode
            do1,   // used as MISO in Standard SPI mode
            do2: _,
            do3: _,
            mut rst,
            mut en,
            tp_sda: _,
            tp_scl: _,
        } = display_pins;

        // Power / enable: keep AMOLED "EN" high (some boards gate panel power here)
        en.set_high();

        // Optional manual reset pulse (the driver will reset too; this just helps first bring-up)
        rst.set_low();
        for _ in 0..10_000 { core::hint::spin_loop(); }
        rst.set_high();
        for _ in 0..10_000 { core::hint::spin_loop(); }

        // SPI @ 20 MHz first (bump to 40 MHz once stable), Mode 0
        let spi_cfg = SpiConfig::default()
            .with_frequency(Rate::from_hz(10_000_000))
            .with_mode(Mode::_0);

        // Create SPI2 with SCK=clk and MOSI=do0 (no MISO)
        let spi = Spi::new(spi2, spi_cfg).unwrap()
            .with_sck(clk)
            .with_mosi(do0)
            .with_miso(do1);  // <-- add this so reads work


        // Manage CS automatically per transfer
        let spi_dev = ExclusiveDevice::new(spi, cs, NoDelay).unwrap();

        // Initialize CO5300 (no D/C pin; 0x02 framing; 466x466)
        let mut delay = SpinDelay;
        Co5300Display::new(
            spi_dev,
            Some(rst),
            &mut delay,
            CO5300_WIDTH,
            CO5300_HEIGHT,
        ).expect("CO5300 init")
    }
}


#[cfg(feature = "devkit-esp32s3-disp128")]
pub use gc9a01_backend::{setup_display, DisplayType};

#[cfg(feature = "esp32s3-disp143Old")]
pub use co5300_backend::{setup_display, DisplayType};
