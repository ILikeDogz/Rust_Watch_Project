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
    digital::OutputPin,
    spi::SpiDevice,
};

use embedded_hal_bus::spi::{ExclusiveDevice, NoDelay};
use esp_hal::gpio::Output;
use esp_hal::spi::master::Spi;
use esp_hal::Blocking;

use embedded_hal::spi::Operation;
use heapless::Vec;

// Public constants so the rest of your code can adopt 466×466 easily.
pub const CO5300_WIDTH: u16 = 466;
pub const CO5300_HEIGHT: u16 = 466;

pub fn ramwr_stream<SD: SpiDevice<u8>>(
    spi: &mut SD,
    chunks: &[&[u8]],   // a list of pixel slices to send back-to-back
) {
    
    let hdr = [0x02, 0x00, 0x2C, 0x00];
    // Build one transaction: header + N data chunks (CS stays asserted)
    // If you don't have heapless, just do two writes: header then a big concat buffer.
    let mut ops: Vec<Operation<'_, u8>, 512> = Vec::new();
    ops.push(Operation::Write(&hdr)).ok();
    for &c in chunks {
        ops.push(Operation::Write(c)).ok();
    }
    spi.transaction(&mut ops).ok();
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
    // Allow toggling even alignment from callers (optional)
    pub fn set_align_even(&mut self, on: bool) { self.align_even = on; }

    /// Create + init the panel. Call once at startup.
    ///
    /// * `spi` - an SPI device with CS control (e.g., `embedded_hal_bus::spi::ExclusiveDevice`)
    /// * `rst` - optional reset pin (recommended to wire)
    /// * `delay` - any `DelayNs` impl (spin delay is fine)
    /// * `width`, `height` - normally 466x466 for this AMOLED
    pub fn new(
        spi: SPI,
        rst: Option<RST>,
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
        if let Some(r) = this.rst.as_mut() {
            r.set_high().map_err(Co5300Error::Gpio)?;
            delay.delay_ms(1);
            r.set_low().map_err(Co5300Error::Gpio)?;
            delay.delay_ms(50);
            r.set_high().map_err(Co5300Error::Gpio)?;
            delay.delay_ms(150);
        }


        // this.cmd(0xFF, &[])?;   // Reset to single-SPI
        this.cmd(0x01, &[])?; // SWRESET

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

        // Set memory access control (orientation)
        this.cmd(0x36, &[0x00])?; 

        // Set full window
        this.cmd(0x2A, &[0x00, 0x00, ((width-1)>>8) as u8, ((width-1)&0xFF) as u8])?;
        this.cmd(0x2B, &[0x00, 0x00, ((height-1)>>8) as u8, ((height-1)&0xFF) as u8])?;
        
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

        if self.align_even {
            x0 &= !1;
            if (x1 & 1) == 0 { x1 = x1.saturating_add(1).min(self.w - 1); }
            if y0 != y1 {
                y0 &= !1;
                if (y1 & 1) == 0 { y1 = y1.saturating_add(1).min(self.h - 1); }
            }
        }

        // Apply panel offsets
        let x0p = x0 + self.x_off;
        let x1p = x1 + self.x_off;
        let y0p = y0 + self.y_off;
        let y1p = y1 + self.y_off;

        let ca = [(x0p >> 8) as u8, (x0p & 0xFF) as u8, (x1p >> 8) as u8, (x1p & 0xFF) as u8];
        let ra = [(y0p >> 8) as u8, (y0p & 0xFF) as u8, (y1p >> 8) as u8, (y1p & 0xFF) as u8];
        self.cmd(0x2A, &ca)?;
        self.cmd(0x2B, &ra)?;
        Ok(())
    }


    /// Write a contiguous buffer of RGB565 big-endian pixels to the current window.
    pub fn write_pixels(&mut self, rgb565_be: &[u8])
        -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    {
        // Use ramwr_stream with a single chunk
        let chunks = [&rgb565_be[..]];
        ramwr_stream(&mut self.spi, &chunks);
        Ok(())
    }

    /// Write a list of pixel rows (each row is &[u8]) in one RAMWR transaction.
    pub fn write_pixels_rows(&mut self, rows: &[&[u8]])
        -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    {
        ramwr_stream(&mut self.spi, rows);
        Ok(())
    }


    /// Convenience: fill a rectangle with a solid color (fast path).
    pub fn fill_rect_solid(&mut self, x: u16, y: u16, w: u16, h: u16, color: Rgb565)
        -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    {
        if w == 0 || h == 0 { return Ok(()); }
        let x1 = x + w - 1;
        let y1 = y + h - 1;
        self.set_window(x, y, x1, y1)?;

        // Prepare one row of the solid color
        let c = color.into_storage().to_be_bytes();

        // Number of bytes per row
        let nbytes = (w as usize) * 2;

        // Build one row buffer filled with the color
        let mut line = [0u8; 466*2];

        // Fill the line buffer with the color
        for i in (0..nbytes).step_by(2) { line[i]=c[0]; line[i+1]=c[1]; }

        // Build a chunk list: one reference to the row per line
        let mut chunks: heapless::Vec<&[u8], 466> = heapless::Vec::new();
        for _ in 0..h {
            chunks.push(&line[..nbytes]).map_err(|_| Co5300Error::OutOfBounds)?;
        }
        self.write_pixels_rows(&chunks)?;
        Ok(())
    }


    // ---- Low-level helpers ----

    #[inline(always)]
    // Force long-header cmd during bring-up
    fn cmd(&mut self, cmd: u8, data: &[u8])
        -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    {
        // let hdr = [0x02, cmd];
        let hdr: [u8; 4] = [0x02, 0x00, cmd, 0x00];
        if data.is_empty() {
            self.spi.write(&hdr).map_err(|e| {
                // println!("spi.write(CMD sh) err: {:?}", e);
                Co5300Error::Spi(e)
            })
        } else {
            self.spi.transaction(&mut [
                Operation::Write(&hdr),
                Operation::Write(data),
            ]).map_err(|e| {
                // println!("spi.tx(CMD sh+data) err: {:?}", e);
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

// ...existing code...

impl<SPI, RST> Co5300Display<SPI, RST>
where
    SPI: embedded_hal::spi::SpiDevice<u8>,
    RST: embedded_hal::digital::OutputPin,
{
    /// Flush one horizontal run. Pads to even length only if align_even is enabled.
    fn flush_run_row_even(
        &mut self,
        y: u16,
        x0: u16,
        len: u16,
        buf: &mut [u8], // must have +2 spare bytes if padding
    ) {
        if len == 0 { return; }

        // Optional even padding
        let mut out_len = len;
        if self.align_even && (out_len & 1) != 0 {
            let byte_len = (out_len as usize) * 2;
            if byte_len + 1 < buf.len() {
                buf[byte_len]     = buf[byte_len - 2];
                buf[byte_len + 1] = buf[byte_len - 1];
                out_len += 1;
            }
        }

        // Clamp to right edge just in case
        let max_len = self.w.saturating_sub(x0);
        if out_len > max_len {
            out_len = max_len;
        }

        let x1 = x0 + out_len - 1;
        let _ = self.set_window(x0, y, x1, y);
        let byte_len = (out_len as usize) * 2;
        let _ = self.write_pixels(&buf[..byte_len]);
    }
}

// -------------------- embedded-graphics integration --------------------
// ...existing code...

impl<SPI, RST> embedded_graphics::draw_target::DrawTarget for Co5300Display<SPI, RST>
where
    SPI: embedded_hal::spi::SpiDevice<u8>,
    RST: embedded_hal::digital::OutputPin,
{
    type Color = embedded_graphics::pixelcolor::Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<embedded_graphics::pixelcolor::Rgb565>>,
    {
        use embedded_graphics::{pixelcolor::Rgb565, Pixel};

        // One scanline buffer (+2 bytes headroom for optional padding)
        let mut buf: [u8; (466 * 2) + 2] = [0; (466 * 2) + 2];

        // Current run state
        let mut run_len: u16 = 0;
        let mut run_x0: u16 = 0;
        let mut run_y: u16 = 0;

        for Pixel(coord, color) in pixels {
            // Clip
            if coord.x < 0 || coord.y < 0 { continue; }
            let (x, y) = (coord.x as u16, coord.y as u16);
            if x >= self.w || y >= self.h { continue; }

            // Flush if row changes or not contiguous
            if run_len != 0 && (y != run_y || x != run_x0 + run_len) {
                self.flush_run_row_even(run_y, run_x0, run_len, &mut buf);
                run_len = 0;
            }

            // Start new run if needed
            if run_len == 0 {
                run_x0 = x;
                run_y = y;
            }

            // Append pixel (RGB565 big-endian)
            let be = Rgb565::into_storage(color).to_be_bytes();
            let idx = (run_len as usize) * 2;
            if idx + 1 < buf.len() {
                buf[idx] = be[0];
                buf[idx + 1] = be[1];
                run_len = run_len.saturating_add(1);
            } else {
                // Buffer full (shouldn’t happen if width <= 466): flush then add
                self.flush_run_row_even(run_y, run_x0, run_len, &mut buf);
                run_x0 = x;
                run_y = y;
                buf[0] = be[0];
                buf[1] = be[1];
                run_len = 1;
            }
        }

        // Flush remaining run
        self.flush_run_row_even(run_y, run_x0, run_len, &mut buf);

        Ok(())
    }

    fn clear(&mut self, color: embedded_graphics::pixelcolor::Rgb565) -> Result<(), Self::Error> {
        let _ = self.fill_rect_solid(0, 0, self.w, self.h, color);
        Ok(())
    }
}
// ...existing code...

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
    let mut display = Co5300Display::new(spi, rst, delay, CO5300_WIDTH, CO5300_HEIGHT)?;
    display.set_window(0, 0, CO5300_WIDTH - 1, CO5300_HEIGHT - 1)?;
    Ok(display)
}

// This matches your wiring: Spi<'a, Blocking> + CS pin + NoDelay
pub type SpiDev<'a> = ExclusiveDevice<Spi<'a, Blocking>, Output<'a>, NoDelay>;

// Expose a single ready-to-use display type that ui.rs can alias:
pub type DisplayType<'a> = Co5300Display<SpiDev<'a>, Output<'a>>;
