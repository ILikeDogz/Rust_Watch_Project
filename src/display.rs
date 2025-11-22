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
    spi::master::Config,
    spi::Mode,
    time::Rate,
    Blocking,
    timer::systimer::{SystemTimer, Unit},
};

use crate::wiring::DisplayPins;


// A delay provider that uses the ESP32-S3's high-resolution SystemTimer.
pub struct TimerDelay;

impl embedded_hal::delay::DelayNs for TimerDelay {
    #[inline]
    fn delay_ns(&mut self, ns: u32) {
        let ticks_per_sec = SystemTimer::ticks_per_second();
        let start = SystemTimer::unit_value(Unit::Unit0); // <-- FIXED
        // Calculate required ticks, rounding up to ensure at least minimum delay
        let delta_ticks = (ns as u64 * ticks_per_sec).div_ceil(1_000_000_000);
        let end_ticks = start.saturating_add(delta_ticks);

        while SystemTimer::unit_value(Unit::Unit0) < end_ticks { // <-- FIXED
            core::hint::spin_loop();
        }
    }

    #[inline]
    fn delay_us(&mut self, us: u32) {
        let ticks_per_sec = SystemTimer::ticks_per_second();
        let start = SystemTimer::unit_value(Unit::Unit0); // <-- FIXED
        let delta_ticks = (us as u64 * ticks_per_sec).div_ceil(1_000_000);
        let end_ticks = start.saturating_add(delta_ticks);

        while SystemTimer::unit_value(Unit::Unit0) < end_ticks { // <-- FIXED
            core::hint::spin_loop();
        }
    }

    #[inline]
    fn delay_ms(&mut self, ms: u32) {
        let ticks_per_sec = SystemTimer::ticks_per_second();
        let start = SystemTimer::unit_value(Unit::Unit0); // <-- FIXED
        let delta_ticks = (ms as u64 * ticks_per_sec).div_ceil(1_000);
        let end_ticks = start.saturating_add(delta_ticks);

        while SystemTimer::unit_value(Unit::Unit0) < end_ticks { // <-- FIXED
            core::hint::spin_loop();
        }
    }
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
        let mut delay = TimerDelay;

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
    use super::*;
    use embedded_hal::delay::DelayNs;
    use esp_hal::{
        dma::{DmaRxBuf, DmaTxBuf},
        dma_buffers,
        spi::master::{Spi, SpiDmaBus},
    };
    use crate::co5300::{self, Co5300Display, RawSpiDev};

    pub type DisplayType<'a> = Co5300Display<'a, Output<'a>>;

    pub fn setup_display<'a>(
        display_pins: DisplayPins<'a>,
        fb: &'a mut [u16],
    ) -> DisplayType<'a> {
        let DisplayPins {
            spi2,
            cs,
            clk,
            do0,
            do1,
            do2 ,
            do3,
            rst,
            mut en,
            tp_sda: _,
            tp_scl: _,
            dma_ch0,
        } = display_pins;

        let mut delay = TimerDelay; 

        // Power up panel

        // quick toggle EN pin
        en.set_low();
        delay.delay_ms(10);     // ensure it’s really off
        en.set_high();
        delay.delay_ms(100);    // give panel power rails time to stabilise

        // // SPI @ 40 MHz in datasheet, Mode 0, known stable, up to 80 MHz overclock might work but is unstable
        // let spi = Spi::new(
        //     spi2,
        //     Config::default()
        //         .with_frequency(Rate::from_hz(80_000_000))
        //         .with_mode(Mode::_0),
        // )
        // .unwrap()
        // .with_sck(clk)
        // .with_mosi(do0)
        // // .with_miso(do1)
        // .with_dma(dma_ch0);

        use esp_hal::spi::master::DataMode; // we'll need this later

        // 60 MHz quad; adjust if instability shows up
        let spi = Spi::new(
            spi2,
            Config::default()
                .with_frequency(Rate::from_hz(80_000_000))
                .with_mode(Mode::_0),
        )
        .unwrap()
        .with_sck(clk)
        .with_sio0(do0)
        .with_sio1(do1)
        .with_sio2(do2)
        .with_sio3(do3)
        .with_dma(dma_ch0);


        let (rx_buf, rx_desc, tx_buf, tx_desc) = dma_buffers!(4096, 65536);
        let rx = DmaRxBuf::new(rx_desc, rx_buf).unwrap();
        let tx = DmaTxBuf::new(tx_desc, tx_buf).unwrap();

        let spi_bus = spi.with_buffers(rx, tx);
        let raw = RawSpiDev { bus: spi_bus, cs };

        co5300::new_with_defaults(raw, Some(rst), &mut delay, fb)
            .expect("CO5300 init failed")

    }
}


#[cfg(feature = "devkit-esp32s3-disp128")]
pub use gc9a01_backend::{setup_display, DisplayType};

#[cfg(feature = "esp32s3-disp143Oled")]
pub use co5300_backend::{setup_display, DisplayType};
