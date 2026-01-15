// Display setup and initialization module.
//
// - `setup_display` picks the right backend based on features.
// - Reuses your SpinDelay and DisplayPins wiring.
// - GC9A01 path uses mipidsi (240x240, D/C).
// - CO5300 path uses your no_std driver (466x466, no D/C, 0x02 framing).

use esp_backtrace as _;

#[cfg(feature = "esp32s3-disp143Oled")]
extern crate alloc;
#[cfg(feature = "esp32s3-disp143Oled")]
use alloc::{boxed::Box, vec};

#[cfg(feature = "devkit-esp32s3-disp128")]
use esp_hal::ram;

// ------------------------- Common imports -------------------------
use esp_hal::{gpio::Output, spi::master::Config, spi::Mode, time::Rate};

use crate::wiring::DisplayPins;
#[cfg(feature = "devkit-esp32s3-disp128")]
#[ram]
static mut DISPLAY_BUF: [u8; 1024] = [0; 1024];

use embedded_graphics::pixelcolor::Rgb565;
#[cfg(feature = "devkit-esp32s3-disp128")]
use embedded_graphics::{
    image::{Image, ImageRawBE},
    prelude::{Point, Size},
    primitives::{Line, PrimitiveStyle, Rectangle},
    Drawable,
};

pub use crate::hal::timer::TimerDelay;

// Optional fast-path operations used by the CO5300 backend.
// For backends without a framebuffer, these are emulated with embedded-graphics.
pub trait FastPanelOps {
    type Error;
    fn flush_rect_even(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) -> Result<(), Self::Error>;
    fn draw_line_fb(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: embedded_graphics::pixelcolor::Rgb565,
        stroke: u8,
    ) -> Option<(u16, u16, u16, u16)>;
    fn fill_rect_fb(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: embedded_graphics::pixelcolor::Rgb565,
    );
    fn fill_rect_solid_no_fb(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        color: embedded_graphics::pixelcolor::Rgb565,
    ) -> Result<(), Self::Error>;
    fn write_rect_fb(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        bytes: &[u8],
    ) -> Result<(), Self::Error>;
    fn blit_rect_be_fast(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        bytes: &[u8],
    ) -> Result<(), Self::Error>;
    fn blit_rect_be_fast_no_fb(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        bytes: &[u8],
    ) -> Result<(), Self::Error>;
}

// ==================================================================
// GC9A01 (240x240) backend  — feature: devkit-esp32s3-disp128
// ==================================================================
#[cfg(feature = "devkit-esp32s3-disp128")]
mod gc9a01_backend {
    use super::*;
    use mipidsi::interface::SpiInterface;
    use mipidsi::{
        models::GC9A01,
        options::{ColorInversion, ColorOrder, Orientation, Rotation},
        Builder as DisplayBuilder,
    };

    // Type alias for GC9A01 display
    pub type DisplayType<'a> = mipidsi::Display<
        SpiInterface<'a, ExclusiveDevice<Spi<'a, Blocking>, Output<'a>, NoDelay>, Output<'a>>,
        GC9A01,
        Output<'a>,
    >;

    // Setup GC9A01 display
    pub fn setup_display<'a>(
        display_pins: DisplayPins<'a>,
        display_buf: &'a mut [u8],
    ) -> DisplayType<'a> {
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
        for _ in 0..10000 {
            core::hint::spin_loop();
        }
        lcd_rst.set_high();
        lcd_bl.set_high();

        // SPI @ 40 MHz, Mode 0
        let spi_cfg = Config::default()
            .with_frequency(Rate::from_hz(40_000_000))
            .with_mode(Mode::_0);

        let spi = Spi::new(spi2, spi_cfg)
            .unwrap()
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

// Export GC9A01 display type and setup function
#[cfg(feature = "devkit-esp32s3-disp128")]
pub use gc9a01_backend::{setup_display, DisplayType};

// FastPanelOps implementation for GC9A01 backend, using embedded-graphics
#[cfg(feature = "devkit-esp32s3-disp128")]
impl<'a> FastPanelOps for gc9a01_backend::DisplayType<'a> {
    type Error = ();

    fn flush_rect_even(&mut self, _x0: u16, _y0: u16, _x1: u16, _y1: u16) -> Result<(), Self::Error> {
        Ok(())
    }

    fn draw_line_fb(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: Rgb565,
        stroke: u8,
    ) -> Option<(u16, u16, u16, u16)> {
        let _ = Line::new(Point::new(x0, y0), Point::new(x1, y1))
            .into_styled(PrimitiveStyle::with_stroke(color, stroke.max(1) as u32))
            .draw(self);
        let half = (stroke.max(1) as i32) / 2;
        let minx = (x0.min(x1) - half).max(0) as u16;
        let miny = (y0.min(y1) - half).max(0) as u16;
        let maxx = (x0.max(x1) + half).max(0) as u16;
        let maxy = (y0.max(y1) + half).max(0) as u16;
        Some((minx, miny, maxx, maxy))
    }

    fn fill_rect_fb(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb565) {
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        if x1 < x0 || y1 < y0 {
            return;
        }
        let w = (x1 - x0 + 1).max(0) as u32;
        let h = (y1 - y0 + 1).max(0) as u32;
        let _ = Rectangle::new(Point::new(x0, y0), Size::new(w, h))
            .into_styled(PrimitiveStyle::with_fill(color))
            .draw(self);
    }

    fn fill_rect_solid_no_fb(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        color: Rgb565,
    ) -> Result<(), Self::Error> {
        let x1 = x.saturating_add(w.saturating_sub(1)) as i32;
        let y1 = y.saturating_add(h.saturating_sub(1)) as i32;
        self.fill_rect_fb(x as i32, y as i32, x1, y1, color);
        Ok(())
    }

    fn write_rect_fb(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        bytes: &[u8],
    ) -> Result<(), Self::Error> {
        let raw = ImageRawBE::<Rgb565>::new(bytes, w as u32);
        let _ = Image::new(&raw, Point::new(x as i32, y as i32)).draw(self);
        Ok(())
    }

    fn blit_rect_be_fast(
        &mut self,
        _x: u16,
        _y: u16,
        _w: u16,
        _h: u16,
        _bytes: &[u8],
    ) -> Result<(), Self::Error> {
        Err(())
    }

    fn blit_rect_be_fast_no_fb(
        &mut self,
        _x: u16,
        _y: u16,
        _w: u16,
        _h: u16,
        _bytes: &[u8],
    ) -> Result<(), Self::Error> {
        Err(())
    }
}

// Initialize GC9A01 display
#[cfg(feature = "devkit-esp32s3-disp128")]
pub fn init_display<'a>(display_pins: DisplayPins<'a>) -> DisplayType<'a> {
    // Safe because DISPLAY_BUF is only used here.
    unsafe { setup_display(display_pins, &mut DISPLAY_BUF) }
}

// ==================================================================
// CO5300 (466x466) backend — feature: esp32s3-disp143Oled
// ==================================================================
#[cfg(feature = "esp32s3-disp143Oled")]
mod co5300_backend {
    use super::*;
    use crate::drivers::co5300::{self, Co5300Display, RawSpiDev};
    use embedded_hal::delay::DelayNs;
    use esp_hal::{
        dma::{DmaRxBuf, DmaTxBuf},
        dma_buffers,
        spi::master::Spi,
    };

    // Type alias for CO5300 display
    pub type DisplayType<'a> = Co5300Display<'a, Output<'a>>;

    // Setup CO5300 display
    pub fn setup_display<'a>(display_pins: DisplayPins<'a>, fb: &'a mut [u16]) -> DisplayType<'a> {
        let DisplayPins {
            spi2,
            cs,
            clk,
            do0,
            do1,
            do2,
            do3,
            rst,
            mut en,
            dma_ch0,
        } = display_pins;

        let mut delay = TimerDelay;

        // Power up panel

        // quick toggle EN pin
        en.set_low();
        delay.delay_ms(10); // ensure it’s really off
        en.set_high();
        delay.delay_ms(100); // give panel power rails time to stabilise

        // SPI @ 40 MHz in datasheet, Mode 0, known stable, up to 80 MHz overclock might work but might be unstable
        // Vibing at 80 MHz here for better performance
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

        co5300::new_with_defaults(raw, Some(rst), &mut delay, fb).expect("CO5300 init failed")
    }
}

// Export CO5300 display type and setup function
#[cfg(feature = "esp32s3-disp143Oled")]
pub use co5300_backend::{setup_display, DisplayType};

// FastPanelOps implementation for CO5300 backend, for compatibility as these are implemented natively
#[cfg(feature = "esp32s3-disp143Oled")]
impl<'a> FastPanelOps for co5300_backend::DisplayType<'a> {
    type Error = ();

    fn flush_rect_even(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) -> Result<(), Self::Error> {
        co5300_backend::DisplayType::flush_rect_even(self, x0, y0, x1, y1).map_err(|_| ())
    }

    fn draw_line_fb(
        &mut self,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: Rgb565,
        stroke: u8,
    ) -> Option<(u16, u16, u16, u16)> {
        co5300_backend::DisplayType::draw_line_fb(self, x0, y0, x1, y1, color, stroke)
    }

    fn fill_rect_fb(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Rgb565) {
        co5300_backend::DisplayType::fill_rect_fb(self, x0, y0, x1, y1, color)
    }

    fn fill_rect_solid_no_fb(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        color: Rgb565,
    ) -> Result<(), Self::Error> {
        co5300_backend::DisplayType::fill_rect_solid_no_fb(self, x, y, w, h, color).map_err(|_| ())
    }

    fn write_rect_fb(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        bytes: &[u8],
    ) -> Result<(), Self::Error> {
        co5300_backend::DisplayType::write_rect_fb(self, x, y, w, h, bytes).map_err(|_| ())
    }

    fn blit_rect_be_fast(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        bytes: &[u8],
    ) -> Result<(), Self::Error> {
        co5300_backend::DisplayType::blit_rect_be_fast(self, x, y, w, h, bytes).map_err(|_| ())
    }

    fn blit_rect_be_fast_no_fb(
        &mut self,
        x: u16,
        y: u16,
        w: u16,
        h: u16,
        bytes: &[u8],
    ) -> Result<(), Self::Error> {
        co5300_backend::DisplayType::blit_rect_be_fast_no_fb(self, x, y, w, h, bytes)
            .map_err(|_| ())
    }
}

// Initialize CO5300 display
#[cfg(feature = "esp32s3-disp143Oled")]
pub fn init_display<'a>(display_pins: DisplayPins<'a>) -> DisplayType<'a> {
    const W: usize = 466;
    let fb: &'static mut [u16] = Box::leak(vec![0u16; W * W].into_boxed_slice());
    setup_display(display_pins, fb)
}
