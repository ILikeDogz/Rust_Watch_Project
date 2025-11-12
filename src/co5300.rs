
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
// Geometry: panel is 466 x 466 logical pixels (square).

use core::fmt;

use embedded_graphics::primitives::Rectangle;
use embedded_graphics::{
    pixelcolor::Rgb565,
    prelude::*,
    pixelcolor::raw::RawU16,
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

extern crate alloc;
use alloc::vec::Vec;

// Public constants so the rest of your code can adopt 466×466 easily.
pub const CO5300_WIDTH: u16 = 466;
pub const CO5300_HEIGHT: u16 = 466;
const RAMWR_OPCODE: u8 = 0x2C;

use embedded_graphics::prelude::IntoStorage;

/// Low-level command send helper (basically holds this entire thing together)
pub fn ramwr_stream<SD: SpiDevice<u8>>(
    spi: &mut SD,
    chunks: &[&[u8]],
) -> Result<(), SD::Error> {
    use embedded_hal::spi::Operation;

    let hdr: [u8; 4] = [0x02, 0x00, RAMWR_OPCODE, 0x00];

    if chunks.len() == 1 {
        if let Some(&data) = chunks.first() {
            let mut ops: heapless::Vec<Operation<'_, u8>, 2> = heapless::Vec::new();
            ops.push(Operation::Write(&hdr)).ok();
            ops.push(Operation::Write(data)).ok();
            return spi.transaction(&mut ops);
        }
        return Ok(());
    }

    // Increase capacity so we can write whole images (<= 2 ops per row + 1 hdr)
    let mut ops: heapless::Vec<Operation<'_, u8>, 1024> = heapless::Vec::new();
    ops.push(Operation::Write(&hdr)).ok();

    for &c in chunks {
        ops.push(Operation::Write(c)).ok();
    }

    spi.transaction(&mut ops)
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
pub struct Co5300Display<'fb,SPI, RST> {
    pub spi: SPI,
    rst: Option<RST>,
    w: u16,
    h: u16,
    x_off: u16,
    y_off: u16,
    align_even: bool,
    fb: &'fb mut [u16], // framebuffer storage
}


impl<'fb, SPI, RST> Co5300Display<'fb, SPI, RST>
where
    // embedded-hal 1.0 `SpiDevice<u8>` so we can do atomic CS-asserted transfers.
    SPI: SpiDevice<u8>,
    RST: OutputPin,
{
    // Allow toggling even alignment from callers (optional)
    pub fn set_align_even(&mut self, on: bool) { self.align_even = on; }

    pub fn is_align_even(&self) -> bool { self.align_even }

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
        fb: &'fb mut [u16],
    ) -> Result<Self, Co5300Error<SPI::Error, RST::Error>> {

        // Validate FB size matches WxH (RGB565)
        let expected = (width as usize) * (height as usize);
        if fb.len() != expected {
            return Err(Co5300Error::OutOfBounds);
        }

        // Construct with NO offsets and NO even alignment for now
        let mut this = Self {
            spi,
            rst,
            w: width,
            h: height,
            x_off: 0x0006,
            y_off: 0x0000,
            align_even: false,
            fb,
        };

        // Hard reset sequence
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
        
        this.fb.fill(0); // clear FB
        Ok(this)
    }

    // Panel width in pixels.
    #[inline]
    pub fn width(&self) -> u16 { self.w }

    // Panel height in pixels.
    #[inline]
    pub fn height(&self) -> u16 { self.h }

    // Panel Size
    pub fn size(&self) -> (u16, u16) { (self.w, self.h) }

    // Set the active drawing window (inclusive coordinates).
    // Use before streaming pixels with `write_pixels`.
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
            y0 &= !1;
            if (y1 & 1) == 0 { y1 = y1.saturating_add(1).min(self.h - 1); }
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

    // Raw window set (no even expansion, still applies panel offsets)
    fn set_window_raw(
        &mut self,
        x0: u16, y0: u16,
        x1: u16, y1: u16,
    ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
        if x0 > x1 || y0 > y1 || x1 >= self.w || y1 >= self.h {
            return Err(Co5300Error::OutOfBounds);
        }
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


    // TODO LETS TEST THESE, also I want brightness control
    // //---- Power control ---- all untested:
    // // Quick blank/unblank without sleep
    // pub fn display_off(&mut self) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
    //     self.cmd(0x28, &[])?; // DISP OFF
    //     Ok(())
    // }

    // pub fn display_on(&mut self, delay: &mut impl embedded_hal::delay::DelayNs)
    //     -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    // {
    //     self.cmd(0x29, &[])?; // DISP ON
    //     delay.delay_ms(10);   // small settle before first RAMWR
    //     Ok(())
    // }

    // // Deep sleep control
    // pub fn sleep_in(&mut self, delay: &mut impl embedded_hal::delay::DelayNs)
    //     -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    // {
    //     self.cmd(0x10, &[])?; // SLP IN
    //     delay.delay_ms(120);
    //     Ok(())
    // }

    // pub fn sleep_out(&mut self, delay: &mut impl embedded_hal::delay::DelayNs)
    //     -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    // {
    //     self.cmd(0x11, &[])?; // SLP OUT
    //     delay.delay_ms(120);
    //     Ok(())
    // }

    //     // Convenience: full disable (blank + sleep)
    // pub fn disable(&mut self, delay: &mut impl embedded_hal::delay::DelayNs)
    //     -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    // {
    //     self.display_off()?;
    //     self.sleep_in(delay)?;
    //     Ok(())
    // }

    // // Convenience: full enable (wake + on + re-assert opts if needed)
    // pub fn enable(&mut self, delay: &mut impl embedded_hal::delay::DelayNs)
    //     -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    // {
    //     self.sleep_out(delay)?;

    //     // Some panels lose format/orientation in sleep; re-assert if needed
    //     self.cmd(0x3A, &[0x55])?; // RGB565
    //     self.cmd(0x36, &[0x00])?; // MADCTL (adjust if you rotate)

    //     self.display_on(delay)?;
    //     // Optionally restore brightness
    //     // self.cmd(0x51, &[0xFF])?;
    //     Ok(())
    // }


    /// Write a list of pixel rows (each row is &[u8]) in one RAMWR transaction.
    pub fn write_pixels_rows(&mut self, rows: &[&[u8]])
        -> Result<(), Co5300Error<SPI::Error, RST::Error>>
    {
        ramwr_stream(&mut self.spi, rows).map_err(Co5300Error::Spi)
    }


    // temporary for prototyping
    pub fn write_2x2(
        &mut self,
        x: u16,
        y: u16,
        color_1: Rgb565,
        color_2: Rgb565,
        color_3: Rgb565,
        color_4: Rgb565,
    ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
        if x >= self.w || y >= self.h || x + 1 >= self.w || y + 1 >= self.h {
            return Err(Co5300Error::OutOfBounds);
        }
        // Align to even x (panel quirk-friendly)
        let x0 = x & !1;
        let y0 = y & !1;

        if x0 + 1 >= self.w || y0 + 1 >= self.h {
            return Err(Co5300Error::OutOfBounds);
        }

        self.set_window_raw(x0, y0, x0 + 1, y0 + 1)?;

        let a = color_1.into_storage().to_be_bytes();
        let b = color_2.into_storage().to_be_bytes();
        let c = color_3.into_storage().to_be_bytes();
        let d = color_4.into_storage().to_be_bytes();

        let row0 = [a[0], a[1], b[0], b[1]];
        let row1 = [c[0], c[1], d[0], d[1]];

        let rows: [&[u8]; 2] = [&row0, &row1];
        self.write_pixels_rows(&rows)
    }

    // pub fn write_logical_pixel(
    // &mut self,
    // x: u16,
    // y: u16,
    // color: Rgb565,
    // ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
    //     if x >= self.w || y >= self.h {
    //         return Ok(());
    //     }

    //     // Tile origin (even coords)
    //     let ax = x & !1;
    //     let ay = y & !1;

    //     // Tile size (clamp at panel edges)
    //     let tile_w: u16 = if ax + 1 < self.w { 2 } else { 1 };
    //     let tile_h: u16 = if ay + 1 < self.h { 2 } else { 1 };

    //     // Load existing tile from FB
    //     let fbw = self.w as usize;
    //     let base = (ay as usize) * fbw + (ax as usize);

    //     let mut a = self.fb[base];
    //     let mut b = if tile_w == 2 { self.fb[base + 1] } else { a };
    //     let mut c = if tile_h == 2 { self.fb[base + fbw] } else { a };
    //     let mut d = if tile_w == 2 && tile_h == 2 { self.fb[base + fbw + 1] } else { c };

    //     // Replace only the selected quadrant
    //     let new16 = color.into_storage();
    //     match ((x & 1) as u16, (y & 1) as u16) {
    //         (0, 0) => { a = new16; self.fb[base] = new16; }
    //         (1, 0) => { b = new16; if tile_w == 2 { self.fb[base + 1] = new16; } }
    //         (0, 1) => { c = new16; if tile_h == 2 { self.fb[base + fbw] = new16; } }
    //         (1, 1) => {
    //             d = new16;
    //             if tile_w == 2 && tile_h == 2 {
    //                 self.fb[base + fbw + 1] = new16;
    //             }
    //         }
    //         _ => {}
    //     }

    //     // Build rows
    //     let a_bytes = a.to_be_bytes();
    //     let b_bytes = b.to_be_bytes();
    //     let c_bytes = c.to_be_bytes();
    //     let d_bytes = d.to_be_bytes();

    //     let mut row0 = [0u8; 4];
    //     let mut row1 = [0u8; 4];

    //     row0[0] = a_bytes[0]; 
    //     row0[1] = a_bytes[1];

    //     // Fill in the second pixel if it's a 2x2 tile, otherwise edge
    //     if tile_w == 2 { row0[2] = b_bytes[0]; row0[3] = b_bytes[1]; }

    //     row1[0] = c_bytes[0]; 
    //     row1[1] = c_bytes[1];

    //     // Fill in the second pixel if it's a 2x2 tile, otherwise edge
    //     if tile_w == 2 && tile_h == 2 { row1[2] = d_bytes[0]; row1[3] = d_bytes[1]; }
    //     if tile_w == 2 { row0[2] = b_bytes[0]; row0[3] = b_bytes[1]; }

    //     row1[0] = c_bytes[0]; 
    //     row1[1] = c_bytes[1];
    //     // Fill in the second pixel if it's a 2x2 tile, otherwise edge
    //     if tile_w == 2 && tile_h == 2 { row1[2] = d_bytes[0]; row1[3] = d_bytes[1]; }

    //     // Program exact tile window (no even expansion)
    //     self.set_window_raw(ax, ay, ax + tile_w - 1, ay + tile_h - 1)?;

    //     // Stream tile
    //     let rows: [&[u8]; 2] = [
    //         &row0[..(tile_w as usize) * 2],
    //         &row1[..(tile_h as usize) * (tile_w as usize) * 2 / (tile_h as usize)],
    //     ];
    //     let rows_slice = if tile_h == 2 { &rows[..] } else { &rows[..1] };
    //     self.write_pixels_rows(rows_slice)
    // }

    // Flush a 2-row band from FB with even-aligned X and Y (single RAMWR for the band)
    fn flush_fb_row_even(
        &mut self,
        y: u16,
        x0: u16,
        x1: u16,
    ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
        if y >= self.h || x0 > x1 || x0 >= self.w { return Ok(()); }

        // Align X to even width
        let mut ax0 = x0 & !1;
        let mut ax1 = x1 | 1;
        ax1 = ax1.min(self.w - 1);

        // Align Y to even start and make height=2
        let y0 = y & !1;
        let y1 = (y0 + 1).min(self.h - 1);

        let ew = (ax1 - ax0 + 1) as usize;
        let fbw = self.w as usize;

        // 2 rows * ew * 2 bytes
        let mut band = [0u8; 466 * 2 * 2];

        // Row y0
        let row0_base = (y0 as usize) * fbw + (ax0 as usize);
        for i in 0..ew {
            let v = self.fb[row0_base + i].to_be_bytes();
            band[i * 2] = v[0];
            band[i * 2 + 1] = v[1];
        }
        // Row y1
        let row1_base = (y1 as usize) * fbw + (ax0 as usize);
        let off = ew * 2;
        for i in 0..ew {
            let v = self.fb[row1_base + i].to_be_bytes();
            band[off + i * 2] = v[0];
            band[off + i * 2 + 1] = v[1];
        }

        self.set_window_raw(ax0, y0, ax1, y1)?;
        self.write_pixels_rows(&[&band[..ew * 4]])
    }

    // Flush an FB rectangle, forcing even start/end (2x2 tiles), using raw window.
    fn flush_fb_rect_even(
        &mut self,
        x0: u16,
        y0: u16,
        x1: u16,
        y1: u16,
    ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
        if x0 > x1 || y0 > y1 || x0 >= self.w || y0 >= self.h {
            return Ok(());
        }

        // Align to hardware 2x2 granularity
        let mut ax0 = x0 & !1;
        let mut ay0 = y0 & !1;
        let mut ax1 = x1 | 1;
        let mut ay1 = y1 | 1;

        ax1 = ax1.min(self.w - 1);
        ay1 = ay1.min(self.h - 1);

        let ew = ax1 - ax0 + 1;
        let eh = ay1 - ay0 + 1;

        // Build one contiguous BE buffer (single RAMWR)
        let mut buf: Vec<u8> = Vec::with_capacity((ew as usize) * (eh as usize) * 2);
        let fbw = self.w as usize;

        for y in ay0..=ay1 {
            let row_base = (y as usize) * fbw + (ax0 as usize);
            for x in 0..(ew as usize) {
                let v = self.fb[row_base + x].to_be_bytes();
                buf.push(v[0]);
                buf.push(v[1]);
            }
        }

        self.set_window_raw(ax0, ay0, ax1, ay1)?;
        self.write_pixels_rows(&[&buf])?;
        Ok(())
    }
    
    // /// Convenience: 2x2 solid color tile.
    // pub fn write_2x2_solid(
    //     &mut self,
    //     x: u16,
    //     y: u16,
    //     color: Rgb565,
    // ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
    //     self.write_2x2(x, y, color, color, color, color)
    // }

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

    // Fast path: stream a BE RGB565 rectangle at (x0,y0).
    // `data` is w*h*2 bytes, row-major, big-endian.
    pub fn blit_rect_be_fast(
        &mut self,
        x0: u16,
        y0: u16,
        w:  u16,
        h:  u16,
        data: &[u8],
    ) -> Result<(), Co5300Error<SPI::Error, RST::Error>> {
        if w == 0 || h == 0 { return Ok(()); }
        if x0 >= self.w || y0 >= self.h { return Ok(()); }

        // Clip input to panel
        let w = w.min(self.w.saturating_sub(x0));
        let h = h.min(self.h.saturating_sub(y0));
        if w == 0 || h == 0 { return Ok(()); }
        if data.len() != (w as usize) * (h as usize) * 2 { return Ok(()); }

        // Align start to even (left/top pad) and expand end to even (right/bottom pad)
        let ax0 = x0 & !1;
        let ay0 = y0 & !1;
        let ax1 = (x0 + w - 1) | 1;
        let ay1 = (y0 + h - 1) | 1;

        let ax1 = ax1.min(self.w - 1);
        let ay1 = ay1.min(self.h - 1);

        // Build one contiguous BE buffer covering [ax0..ax1] x [ay0..ay1]
        let ew = (ax1 - ax0 + 1) as usize;
        let eh = (ay1 - ay0 + 1) as usize;

        let fbw = self.w as usize;

        let mut buf: Vec<u8> = Vec::with_capacity(ew * eh * 2);

        // Source row stride in input data
        let src_stride = (w as usize) * 2;

        for ry in 0..eh {
            let y = ay0 + (ry as u16);

            // Are we inside the source rect vertically?
            let in_src_y = y >= y0 && y < y0 + h;
            let src_row_off = if in_src_y {
                ((y - y0) as usize) * src_stride
            } else {
                0 // unused if not in_src_y
            };

            for rx in 0..ew {
                let x = ax0 + (rx as u16);

                let in_src_x = x >= x0 && x < x0 + w;

                if in_src_x && in_src_y {
                    // Take from source image
                    let sx = (x - x0) as usize;
                    let off = src_row_off + sx * 2;
                    let hi = data[off];
                    let lo = data[off + 1];
                    buf.push(hi);
                    buf.push(lo);

                    // Update FB for source pixels
                    self.fb[(y as usize) * fbw + (x as usize)] = u16::from_be_bytes([hi, lo]);
                } else {
                    // Padding region: preserve current framebuffer pixel
                    let v = self.fb[(y as usize) * fbw + (x as usize)].to_be_bytes();
                    buf.push(v[0]);
                    buf.push(v[1]);
                    // FB remains unchanged
                }
            }
        }

        // Program raw window and stream once
        self.set_window_raw(ax0, ay0, ax1, ay1)?;
        self.write_pixels_rows(&[&buf])?;
        Ok(())
    }

 
}


// -------------------- embedded-graphics integration --------------------
impl<'fb, SPI, RST> OriginDimensions for Co5300Display<'fb, SPI, RST>
where
    SPI: SpiDevice<u8>,
    RST: OutputPin,
{
    fn size(&self) -> Size {
        Size::new(self.w as u32, self.h as u32)
    }
}

impl<'fb, SPI, RST> embedded_graphics::draw_target::DrawTarget for Co5300Display<'fb, SPI, RST>
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
        use embedded_graphics::{prelude::Point, Pixel};

        // Global bbox only (cheap), and optionally row spans while sparse.
        let mut any = false;
        let mut minx = self.w;
        let mut miny = self.h;
        let mut maxx: u16 = 0;
        let mut maxy: u16 = 0;
        let mut npix: usize = 0;

        #[derive(Copy, Clone)]
        struct RowSpan { y: u16, minx: u16, maxx: u16 }
        let mut spans: heapless::Vec<RowSpan, 512> = heapless::Vec::new();
        let mut track_spans = true;

        for Pixel(p, c) in pixels.into_iter() {
            if p.x < 0 || p.y < 0 { continue; }
            let (x, y) = (p.x as u16, p.y as u16);
            if x >= self.w || y >= self.h { continue; }

            // Update FB single logical pixel
            self.fb[(y as usize) * (self.w as usize) + (x as usize)] = c.into_storage();

            // Global bbox
            if !any {
                any = true;
                minx = x; maxx = x;
                miny = y; maxy = y;
            } else {
                if x < minx { minx = x; }
                if y < miny { miny = y; }
                if x > maxx { maxx = x; }
                if y > maxy { maxy = y; }
            }
            npix += 1;

            // Stop span tracking once enough pixels seen (dense case → bbox flush)
            if track_spans && npix > 2048 {
                track_spans = false;
            }

            if track_spans {
                if let Some(r) = spans.iter_mut().find(|s| s.y == y) {
                    if x < r.minx { r.minx = x; }
                    if x > r.maxx { r.maxx = x; }
                } else {
                    let _ = spans.push(RowSpan { y, minx: x, maxx: x });
                }
            }
        }

        if !any {
            return Ok(());
        }

        // If we didn’t track spans (dense), flush one even-aligned rect.
        if !track_spans {
            let _ = self.flush_fb_rect_even(minx, miny, maxx, maxy);
            return Ok(());
        }

        // Sparse heuristic: compare touched pixels to bbox area
        let bbox_area = ((maxx - minx + 1) as usize) * ((maxy - miny + 1) as usize);
        let use_spans = npix * 8 < bbox_area;

        if use_spans {
            spans.sort_unstable_by_key(|s| s.y);
            for s in spans.into_iter() {
                let _ = self.flush_fb_row_even(s.y, s.minx, s.maxx);
            }
        } else {
            let _ = self.flush_fb_rect_even(minx, miny, maxx, maxy);
        }

        Ok(())
    }


    // FAST PATH: row streaming for images and large fills
    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Rgb565>,
    {
        use embedded_graphics::prelude::*;

        // Clip to panel
        let bounds = Rectangle::new(Point::new(0, 0), Size::new(self.w as u32, self.h as u32));
        let inter = area.intersection(&bounds);
        if inter.size.width == 0 || inter.size.height == 0 {
            // Drain iterator to keep semantics
            let mut it = colors.into_iter();
            let total = (area.size.width as usize) * (area.size.height as usize);
            for _ in 0..total { let _ = it.next(); }
            return Ok(());
        }

        // Precompute skips and takes
        let area_w = area.size.width as usize;
        let area_h = area.size.height as usize;

        let x0 = inter.top_left.x as u16;
        let y0 = inter.top_left.y as u16;
        let take = inter.size.width as usize;
        let inter_h = inter.size.height as usize;

        let left_skip = (inter.top_left.x - area.top_left.x).max(0) as usize;
        let right_skip = area_w.saturating_sub(left_skip + take);
        let top_skip = (inter.top_left.y - area.top_left.y).max(0) as usize;

        // Collect visible span into a PSRAM buffer (W*H*2 bytes, BE RGB565)
        let mut buf: Vec<u8> = Vec::with_capacity(take * inter_h * 2);
        let mut it = colors.into_iter();

        // Skip full rows above intersection
        for _ in 0..top_skip {
            for _ in 0..area_w { let _ = it.next(); }
        }

        // Collect intersecting rows
        for _r in 0..inter_h {
            // Skip left columns
            for _ in 0..left_skip { let _ = it.next(); }
            // Take visible span
            for _ in 0..take {
                if let Some(c) = it.next() {
                    let b = c.into_storage().to_be_bytes();
                    buf.push(b[0]);
                    buf.push(b[1]);
                }
            }
            // Skip right columns
            for _ in 0..right_skip { let _ = it.next(); }
        }

        // Drain rows below to preserve iterator semantics
        let rows_below = area_h.saturating_sub(top_skip + inter_h);
        for _ in 0..rows_below {
            for _ in 0..area_w { let _ = it.next(); }
        }

        // One-window, one-RAMWR fast path
        let _ = self.blit_rect_be_fast(x0, y0, take as u16, inter_h as u16, &buf);
        Ok(())
    }

    fn clear(&mut self, color: embedded_graphics::pixelcolor::Rgb565) -> Result<(), Self::Error> {
        let v = color.into_storage();
        for px in self.fb.iter_mut() { *px = v; }
        let _ = self.fill_rect_solid(0, 0, self.w, self.h, color);
        Ok(())
    }

}

/// Convenience builder that picks common defaults and returns the concrete type.
/// Returning the concrete type lets display.rs use `impl Trait` to erase it later.
pub fn new_with_defaults<'fb, SPI, RST>(
    spi: SPI,
    rst: Option<RST>,
    delay: &mut impl embedded_hal::delay::DelayNs,
    fb: &'fb mut [u16],
) -> Result<Co5300Display<'fb, SPI, RST>, Co5300Error<SPI::Error, RST::Error>>
where
    SPI: embedded_hal::spi::SpiDevice<u8>,
    RST: embedded_hal::digital::OutputPin,
{
    let mut display = Co5300Display::new(spi, rst, delay, CO5300_WIDTH, CO5300_HEIGHT, fb)?;
    display.set_window(0, 0, CO5300_WIDTH - 1, CO5300_HEIGHT - 1)?;
    Ok(display)
}

// This matches your wiring: Spi<'a, Blocking> + CS pin + NoDelay
pub type SpiDev<'a> = ExclusiveDevice<Spi<'a, Blocking>, Output<'a>, NoDelay>;

// Expose a ready-to-use display type (share lifetime with SPI and FB)
pub type DisplayType<'a> = Co5300Display<'a, SpiDev<'a>, Output<'a>>;
