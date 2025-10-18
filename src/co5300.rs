// #![allow(dead_code)]
// #![allow(unused_imports)]

// Minimal CO5300 panel driver (Standard SPI mode, no D/C pin).
// Works with esp-hal (no_std) and embedded-graphics.
//
// Wiring on Waveshare ESP32-S3 Touch AMOLED 1.43” (CO5300):
//   CS  = GPIO9
//   SCK = GPIO10
//   IO0/MOSI = GPIO11
//   (IO1..IO3 unused in Standard SPI mode)
//   RST = GPIO21
//
// Protocol (Standard SPI):
//   Every write begins with 0x02, then one byte CMD, then N data bytes.
//   Example: [0x02, 0x11] -> Sleep Out
//            [0x02, 0x3A, 0x55] -> Pixel Format = 16bpp (RGB565)
//   For memory writes after setting the window:
//            [0x02, 0x32, <pixel stream in RGB565 big-endian>]
//
// Geometry: panel is 466 x 466 logical pixels (square).

use core::fmt;

use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
};
use embedded_hal::{
    delay::DelayNs,
    digital::OutputPin,
    spi::SpiDevice,
};

use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
use esp_hal::gpio::Output;
use esp_hal::spi::master::Spi;
use esp_hal::Blocking;

use embedded_hal::spi::Operation;

// Public constants so the rest of your code can adopt 466×466 easily.
pub const CO5300_WIDTH: u16 = 466;
pub const CO5300_HEIGHT: u16 = 466;

const SHORT_HEADER: bool = false; // flip to true if needed
const DEBUG_SPI: bool = true;

use esp_println::println;

macro_rules! dprintln {
    ($($arg:tt)*) => {
        if DEBUG_SPI { println!($($arg)*); }
    }
}

/// Error type that wraps SPI and GPIO errors.
#[derive(Debug)]
pub enum Co5300Error<SpiE, GpioE> {
    Spi(SpiE),
    Gpio(GpioE),
    OutOfBounds,

}

impl<SpiE: fmt::Debug, GpioE: fmt::Debug> From<SpiE> for Co5300Error<SpiE, GpioE> {
    fn from(e: SpiE) -> Self { Self::Spi(e) }
}

/// A very small CO5300 panel driver speaking the "0x02 + CMD + DATA" SPI framing.
/// No D/C pin is used; CS is handled by the `SpiDevice` implementation.
/// Implements `DrawTarget<Rgb565>` for convenience (per-pixel path is simple but slow).
pub struct Co5300Display<SPI, RST> {
    pub spi: SPI,
    rst: Option<RST>,
    w: u16,
    h: u16,
    x_off: u16,
    y_off: u16,
    align_even: bool,
}


impl<SPI, RST> Co5300Display<SPI, RST>
where
    // embedded-hal 1.0 `SpiDevice<u8>` so we can do atomic CS-asserted transfers.
    SPI: SpiDevice<u8>,
    RST: OutputPin,
{
    /// Create + init the panel. Call once at startup.
    ///
    /// * `spi` - an SPI device with CS control (e.g., `embedded_hal_bus::spi::ExclusiveDevice`)
    /// * `rst` - optional reset pin (recommended to wire)
    /// * `delay` - any `DelayNs` impl (spin delay is fine)
    /// * `width`, `height` - normally 466x466 for this AMOLED
    pub fn new(
        mut spi: SPI,
        mut rst: Option<RST>,
        delay: &mut impl embedded_hal::delay::DelayNs,
        width: u16,
        height: u16,
    ) -> Result<Self, Co5300Error<SPI::Error, RST::Error>> {

        // Construct with NO offsets and NO even alignment for now
        let mut this = Self {
            spi,
            rst,
            w: width,
            h: height,
            x_off: 0x0006,
            y_off: 0x0000,
            align_even: false,

        };

        // Hard reset sequence (keep it)
        dprintln!("RST sequence start");
        if let Some(r) = this.rst.as_mut() {
            r.set_high().map_err(Co5300Error::Gpio)?;
            delay.delay_ms(1);
            r.set_low().map_err(Co5300Error::Gpio)?;
            delay.delay_ms(50);
            r.set_high().map_err(Co5300Error::Gpio)?;
            delay.delay_ms(150);
        }


        // this.cmd(0xFF, &[])?;   // Reset to single-SPI
        delay.delay_ms(10);
        // ==== Init table equivalent ====
        // 0x11 (SLPOUT), no data, 80 ms delay
        this.cmd(0x11, &[])?;
        delay.delay_ms(120);

        // Ensure pixel format = RGB565 (add if panel needs it)
        this.cmd(0x3A, &[0x55])?; // COLMOD: 16bpp

        // 0xC4 0x80
        this.cmd(0xC4, &[0x80])?;

        this.cmd(0x13, &[])?;          // NORMAL DISPLAY MODE

        // 0x53 0x20 (BCTRL), 1 ms delay
        this.cmd(0x53, &[0x20])?;
        delay.delay_ms(1);

        // 0x63 0xFF (vendor enable), 1 ms delay
        this.cmd(0x63, &[0xFF])?;
        delay.delay_ms(1);

        // 0x51 0x00 (brightness 0), 1 ms delay
        this.cmd(0x51, &[0x00])?;
        delay.delay_ms(1);

        // 0x29 (DISPON), 10 ms delay
        this.cmd(0x29, &[])?;
        delay.delay_ms(30);      // <-- add

        // 0x51 0xFF (brightness max)
        this.cmd(0x51, &[0xFF])?;

        // Optional TE and orientation, if desired:
        // this.cmd(0x35, &[0x00])?; // TE ON
        // this.cmd(0x44, &[0x01, 0xD1])?; // TE scanline
        // this.cmd(0x36, &[0x60])?; // MADCTL rotation/mirror
        this.cmd(0x36, &[0x00])?; // no mirror/rotation during bring-up

        // Set full window
        this.cmd(0x2A, &[0x00, 0x00, ((width-1)>>8) as u8, ((width-1)&0xFF) as u8])?;
        this.cmd(0x2B, &[0x00, 0x00, ((height-1)>>8) as u8, ((height-1)&0xFF) as u8])?;

        dprintln!("INIT DONE");
        
        Ok(this)
    }

    /// Panel width in pixels.
    #[inline]
    pub fn width(&self) -> u16 { self.w }

    /// Panel height in pixels.
    #[inline]
    pub fn height(&self) -> u16 { self.h }

    /// Set the active drawing window (inclusive coordinates).
    /// Use before streaming pixels with `write_pixels`.
    pub fn set_window(
        &mut self,
        mut x0: u16, mut y0: u16,
        mut x1: u16, mut y1: u16,
    ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
        if x0 > x1 || y0 > y1 || x1 >= self.w || y1 >= self.h {
            return Err(Co5300Error::OutOfBounds);
        }

        // Optional: enforce even alignment like the C rounder_cb does
        if self.align_even {
            x0 &= !1; y0 &= !1;
            // x1/y1 are inclusive — round "up" to next odd so (x2+1) is even width
            if (x1 & 1) == 0 { x1 = x1.saturating_add(1); }
            if (y1 & 1) == 0 { y1 = y1.saturating_add(1); }
            // Also clamp in case we overflowed the panel size
            x1 = x1.min(self.w - 1);
            y1 = y1.min(self.h - 1);
        }

        // Apply panel offsets
        let x0p = x0 + self.x_off;
        let x1p = x1 + self.x_off;
        let y0p = y0 + self.y_off;
        let y1p = y1 + self.y_off;

        let ca = [ (x0p >> 8) as u8, (x0p & 0xFF) as u8, (x1p >> 8) as u8, (x1p & 0xFF) as u8 ];
        let ra = [ (y0p >> 8) as u8, (y0p & 0xFF) as u8, (y1p >> 8) as u8, (y1p & 0xFF) as u8 ];
        self.cmd(0x2A, &ca)?;
        self.cmd(0x2B, &ra)?;
        Ok(())
    }


    /// Start a memory write (RAMWR 0x2C) then stream big-endian RGB565 pixel bytes.
    /// Call `set_window()` first to define the rectangle.
    pub fn write_pixels(&mut self, rgb565_be: &[u8])
        -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    {
        // Long header + 0x2C (WRITE_COLOR)
        let hdr = [0x02, 0x00, 0x2C, 0x00];
        // let hdr = [0x02, 0x2C];
        dprintln!("PIX lg: hdr={:02X?} bytes={}", hdr, rgb565_be.len());
        self.spi.transaction(&mut [
            embedded_hal::spi::Operation::Write(&hdr),
            embedded_hal::spi::Operation::Write(rgb565_be),
        ]).map_err(|e| {
            println!("spi.tx(PIX lg) err: {:?}", e);
            Co5300Error::Spi(e)
        })
    }


    /// Convenience: fill a rectangle with a solid color (fast path).
    pub fn fill_rect_solid(&mut self, x:u16,y:u16,w:u16,h:u16, color:Rgb565)
        -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    {
        if w == 0 || h == 0 { return Ok(()); }
        let x1 = x + w - 1;
        let y1 = y + h - 1;
        self.set_window(x, y, x1, y1)?;

        let c = color.into_storage().to_be_bytes();
        let mut line = [0u8; 466*2];
        let nbytes = (w as usize) * 2;
        for i in (0..nbytes).step_by(2) { line[i]=c[0]; line[i+1]=c[1]; }

        for _ in 0..h {
            self.write_pixels(&line[..nbytes])?; // header+data in one transaction
        }
        Ok(())
    }


    pub fn read_id(&mut self) -> Result<u8, Co5300Error<SPI::Error, RST::Error>> {
        let mut id = [0u8; 1];
        let hdr = [0x03, 0x00, 0xDA, 0x00];
        // let hdr = [0x03, 0xDA]; // READ ID command
        dprintln!("READ ID hdr={:02X?}", hdr);
        self.spi.transaction(&mut [
            Operation::Write(&hdr),
            Operation::Read(&mut id),
        ]).map_err(|e| {
            println!("spi.tx(read_id) err: {:?}", e);
            Co5300Error::Spi(e)
        })?;
        dprintln!("READ ID -> {:02X}", id[0]);
        Ok(id[0])
    }

    // Optional: read DA/DB/DC
    pub fn read_mipi_ids(&mut self) -> Result<(u8,u8,u8), Co5300Error<SPI::Error, RST::Error>> {
        let mut a=[0]; let mut b=[0]; let mut c=[0];
        for reg in [0xDA, 0xDB, 0xDC] {
            let hdr = [0x03, 0x00, reg, 0x00];
            dprintln!("READ {:02X} hdr={:02X?}", reg, hdr);
            let out = match reg { 0xDA => &mut a[..], 0xDB => &mut b[..], _ => &mut c[..] };
            self.spi.transaction(&mut [
                Operation::Write(&hdr),
                Operation::Read(out),
            ]).map_err(|e| {
                println!("spi.tx(read {:02X}) err: {:?}", reg, e);
                Co5300Error::Spi(e)
            })?;
        }
        dprintln!("MIPI IDs: DA={:02X} DB={:02X} DC={:02X}", a[0], b[0], c[0]);
        Ok((a[0], b[0], c[0]))
    }



    // ---- Low-level helpers ----

    #[inline(always)]
    // Force long-header cmd during bring-up
    fn cmd(&mut self, cmd: u8, data: &[u8])
        -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    {
        // let hdr = [0x02, cmd];
        let hdr: [u8; 4] = [0x02, 0x00, cmd, 0x00];
        dprintln!("CMD sh: {:02X?} data_len={}", hdr, data.len());
        if data.is_empty() {
            self.spi.write(&hdr).map_err(|e| {
                println!("spi.write(CMD sh) err: {:?}", e);
                Co5300Error::Spi(e)
            })
        } else {
            self.spi.transaction(&mut [
                Operation::Write(&hdr),
                Operation::Write(data),
            ]).map_err(|e| {
                println!("spi.tx(CMD sh+data) err: {:?}", e);
                Co5300Error::Spi(e)
            })
        }
    
    }
 
}

// -------------------- embedded-graphics integration --------------------
// NOTE: This is a simple per-pixel fallback so your existing UI compiles
// without refactoring. It’s not fast.
// Prefer using `set_window()` + `write_pixels()` for images/buffers.

impl<SPI, RST> OriginDimensions for Co5300Display<SPI, RST>
where
    SPI: SpiDevice<u8>,
    RST: OutputPin,
{
    fn size(&self) -> Size {
        Size::new(self.w as u32, self.h as u32)
    }
}

impl<SPI, RST> DrawTarget for Co5300Display<SPI, RST>
where
    SPI: SpiDevice<u8>,
    RST: OutputPin,
{
    type Color = Rgb565;
    type Error = Co5300Error<SPI::Error, RST::Error>;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        // Extremely simple: for each pixel, window=1×1 then 2C + 2 bytes.
        // Slow, but works with any embedded-graphics primitives.
        for Pixel(coord, color) in pixels {
            let x = coord.x;
            let y = coord.y;
            if x < 0 || y < 0 { continue; }
            let (x, y) = (x as u16, y as u16);
            if x >= self.w || y >= self.h { continue; }

            self.set_window(x, y, x, y)?;
            let be = color.into_storage().to_be_bytes();
            self.write_pixels(&be)?;
        }
        Ok(())
    }
}

/// Backend's public "Display" name, used by display.rs
// pub type Display<SPI, RST> = Co5300Display<SPI, RST>;

/// Convenience builder that picks common defaults and returns the concrete type.
/// Returning the concrete type lets display.rs use `impl Trait` to erase it later.
pub fn new_with_defaults<SPI, RST>(
    spi: SPI,
    rst: Option<RST>,
    delay: &mut impl embedded_hal::delay::DelayNs,
) -> Result<Co5300Display<SPI, RST>, Co5300Error<SPI::Error, RST::Error>>
where
    SPI: embedded_hal::spi::SpiDevice<u8>,
    RST: embedded_hal::digital::OutputPin,
{
    Co5300Display::new(spi, rst, delay, CO5300_WIDTH, CO5300_HEIGHT)
}

// This matches your wiring: Spi<'a, Blocking> + CS pin + NoDelay
pub type SpiDev<'a> = ExclusiveDevice<Spi<'a, Blocking>, Output<'a>, NoDelay>;

// Expose a single ready-to-use display type that ui.rs can alias:
pub type DisplayType<'a> = Co5300Display<SpiDev<'a>, Output<'a>>;


// -------------------- Integration helpers for esp-hal --------------------
//
// Typical construction with esp-hal:
//
//   use esp_hal::gpio::Output;
//   use esp_hal::spi::{Spi, Blocking};
//   use embedded_hal_bus::spi::ExclusiveDevice;
//   use embedded_hal_bus::spi::NoDelay;
//
//   let spi = Spi::new(peripherals.SPI2, cfg).unwrap()
//       .with_sck(spi_sck)
//       .with_mosi(spi_mosi);
//   let spi_dev = ExclusiveDevice::new(spi, lcd_cs, NoDelay).unwrap();
//
//   // SpinDelay that implements DelayNs (you already have one in display.rs)
//   let mut delay = SpinDelay;
//
//   let mut disp = Co5300Display::new(spi_dev, Some(lcd_rst), &mut delay, CO5300_WIDTH, CO5300_HEIGHT).unwrap();
//
// Fast rectangle blit:
//
//   disp.set_window(x0, y0, x1, y1).unwrap();
//   disp.write_pixels(&rgb565_be_slice).unwrap();
//
// Solid fill:
//
//   disp.fill_rect_solid(0, 0, CO5300_WIDTH, CO5300_HEIGHT, Rgb565::BLACK).unwrap();
//
