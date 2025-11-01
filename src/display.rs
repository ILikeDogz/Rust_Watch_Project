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

// A tiny busy-wait delay that satisfies embedded-hal 1.0 DelayNs.
#[cfg(feature = "devkit-esp32s3-disp128")]
struct SpinDelay;
#[cfg(feature = "devkit-esp32s3-disp128")]
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
#[cfg(feature = "esp32s3-disp143Oled")]
mod co5300_backend {
    use esp_hal::delay::Delay;

    use super::*;

    // Your no_std CO5300 driver
    use crate::co5300::{self, Co5300Display};

    // Concrete type we return for this backend:
    pub type DisplayType<'a> =
        Co5300Display<'a,ExclusiveDevice<Spi<'a, Blocking>, Output<'a>, NoDelay>, Output<'a>>;

    /// OLED setup:
    /// - `spi2`: the SPI2 peripheral (pass `peripherals.SPI2` from main)
    /// - `display_pins`: your new QSPI-style pin bundle
    pub fn setup_display<'a>(
        display_pins: DisplayPins<'a>,
        display_buf: &'a mut [u16],            // required framebuffer
    ) -> DisplayType<'a> {
        // Destructure the pins as defined in your new wiring
        let DisplayPins {
            spi2,
            cs,
            clk,
            do0,   // used as MOSI in Standard SPI mode
            do1,   // used as MISO in Standard SPI mode
            do2: _,
            do3: _,
            rst,
            mut en,
            tp_sda: _,
            tp_scl: _,
        } = display_pins;

        // Power / enable: keep AMOLED "EN" high (some boards gate panel power here)
        let mut delay = Delay::new();

        en.set_high(); delay.delay_ms(50);

        let spi_cfg = SpiConfig::default()
                .with_frequency(Rate::from_hz(40_000_000))  // CO5300 works up to 40 MHz
                .with_mode(Mode::_0);                        // keep Mode0

        let spi = Spi::new(spi2, spi_cfg).unwrap()
                .with_sck(clk)    // GPIO 10
                .with_mosi(do0)   // GPIO 11
                .with_miso(do1);  // GPIO 12 (for reading)

        let spi_dev = ExclusiveDevice::new(spi, cs, NoDelay).unwrap();

        let display = co5300::new_with_defaults(spi_dev, Some(rst), &mut delay, display_buf)
            .expect("CO5300 init failed");

        display
    }
}


#[cfg(feature = "devkit-esp32s3-disp128")]
pub use gc9a01_backend::{setup_display, DisplayType};

#[cfg(feature = "esp32s3-disp143Oled")]
pub use co5300_backend::{setup_display, DisplayType};
