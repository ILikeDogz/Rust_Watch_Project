//! Minimal QMI8658 IMU bring-up and a simple smash detector
//! The register values are conservative defaults for the Waveshare ESP32-S3
//! Touch AMOLED 1.43" board (QMI8658 on the touch I2C bus)

use embedded_hal::i2c;

pub const DEFAULT_I2C_ADDR: u8 = 0x6B; // AD0 pulled high on the Waveshare board

const REG_WHO_AM_I: u8 = 0x00;
const REG_CTRL1: u8 = 0x02; // accel config
const REG_CTRL2: u8 = 0x03; // gyro config
const REG_CTRL7: u8 = 0x08; // power / enable
const REG_CTRL8: u8 = 0x09; // reset/power settings
const REG_ACC_START: u8 = 0x35; // AX_L .. GZ_H

// Expected chip ID for QMI8658. Some revisions report 0x05 or 0x0F; keep it loose.
const WHO_AM_I_FALLBACK: u8 = 0x05;
const WHO_AM_I_ALT: u8 = 0x0F;

#[derive(Clone, Copy, Debug)]
pub struct ImuSample {
    pub accel: [i16; 3],
    pub gyro: [i16; 3],
}

impl ImuSample {
    #[inline]
    pub fn accel_mag_sq(&self) -> i64 {
        self.accel
            .iter()
            .map(|v| {
                let v = *v as i64;
                v * v
            })
            .sum()
    }

    #[inline]
    pub fn gyro_mag_sq(&self) -> i64 {
        self.gyro
            .iter()
            .map(|v| {
                let v = *v as i64;
                v * v
            })
            .sum()
    }
}

#[derive(Debug)]
pub enum ImuError<E> {
    Bus(E),
    BadWhoAmI(u8),
}

impl<E> From<E> for ImuError<E> {
    fn from(e: E) -> Self {
        ImuError::Bus(e)
    }
}

pub struct Qmi8658<I2C> {
    i2c: I2C,
    address: u8,
}

impl<I2C> Qmi8658<I2C>
where
    I2C: i2c::ErrorType + i2c::I2c,
{
    pub fn new(i2c: I2C, address: u8) -> Result<Self, ImuError<I2C::Error>> {
        let mut this = Self { i2c, address };
        this.init()?;
        Ok(this)
    }

    pub fn who_am_i(&mut self) -> Result<u8, ImuError<I2C::Error>> {
        self.read_reg(REG_WHO_AM_I)
    }

    fn init(&mut self) -> Result<(), ImuError<I2C::Error>> {
        let who = self.who_am_i()?;
        if who != WHO_AM_I_FALLBACK && who != WHO_AM_I_ALT {
            // allow init to continue so users can still probe, but surface an error
            return Err(ImuError::BadWhoAmI(who));
        }

        // Soft reset and clear low-power.
        // Ignore errors here to avoid blocking subsequent config steps.
        let _ = self.write_reg(REG_CTRL8, 0x10);

        // Accelerometer: +/-8g, ~1 kHz ODR (0x60 per datasheet examples)
        let _ = self.write_reg(REG_CTRL1, 0x60);
        // Gyro: +/-512 dps, ~1 kHz ODR (0x64 per datasheet examples)
        let _ = self.write_reg(REG_CTRL2, 0x64);

        // Enable accel + gyro, set to Active
        self.write_reg(REG_CTRL7, 0x03)?;

        Ok(())
    }

    fn write_reg(&mut self, reg: u8, val: u8) -> Result<(), ImuError<I2C::Error>> {
        self.i2c
            .write(self.address, &[reg, val])
            .map_err(ImuError::Bus)
    }

    fn read_reg(&mut self, reg: u8) -> Result<u8, ImuError<I2C::Error>> {
        let mut out = [0u8];
        self.i2c
            .write_read(self.address, &[reg], &mut out)
            .map_err(ImuError::Bus)?;
        Ok(out[0])
    }

    pub fn read_sample(&mut self) -> Result<ImuSample, ImuError<I2C::Error>> {
        let mut buf = [0u8; 12];
        self.i2c
            .write_read(self.address, &[REG_ACC_START], &mut buf)
            .map_err(ImuError::Bus)?;

        let accel = [
            i16::from_le_bytes([buf[0], buf[1]]),
            i16::from_le_bytes([buf[2], buf[3]]),
            i16::from_le_bytes([buf[4], buf[5]]),
        ];
        let gyro = [
            i16::from_le_bytes([buf[6], buf[7]]),
            i16::from_le_bytes([buf[8], buf[9]]),
            i16::from_le_bytes([buf[10], buf[11]]),
        ];

        Ok(ImuSample { accel, gyro })
    }

    pub fn into_inner(self) -> I2C {
        self.i2c
    }
}

pub struct SmashDetector {
    threshold_sq: i64,
    rise_threshold_sq: i64,
    freefall_sq: i64,
    gyro_limit_sq: i64,
    // Require one axis to dominate others (to reject swings that are multi-axis noisy)
    axis_ratio_num: i32,
    axis_ratio_den: i32,
    axis_bias_min_dot: i64,
    cooldown_ms: u32,
    last_mag_sq: i64,
    last_freefall: bool,
    last_trigger_ms: u64,
    gravity_dir: [i32; 3],
    gravity_samples: u16,
    baseline_mag_sq: i64,
}

impl SmashDetector {
    pub fn new(threshold_raw: i32, rise_raw: i32, gyro_limit_raw: i32, freefall_raw: i32, cooldown_ms: u32) -> Self {
        Self {
            threshold_sq: (threshold_raw as i64) * (threshold_raw as i64),
            rise_threshold_sq: (rise_raw as i64) * (rise_raw as i64),
            freefall_sq: (freefall_raw as i64) * (freefall_raw as i64),
            gyro_limit_sq: (gyro_limit_raw as i64) * (gyro_limit_raw as i64),
            axis_ratio_num: 0,
            axis_ratio_den: 1,
            axis_bias_min_dot: 0, // disabled until we learn gravity
            cooldown_ms,
            last_mag_sq: 0,
            last_freefall: false,
            last_trigger_ms: 0,
            gravity_dir: [0; 3],
            gravity_samples: 0,
            baseline_mag_sq: 0,
        }
    }

    pub fn default_rough() -> Self {
        // Raw units tuned for observed ~1000 counts per 1g on the Waveshare board (8g range).
        // Re-tighten slightly: ~1.8g threshold, ~0.7g rise, gyro gate ~60k, cooldown 160 ms.
        let mut s = Self::new(1_800, 700, 60_000, 200, 160);
        // Require a dominant axis (at least ~2:1 over others) once enabled.
        s.axis_ratio_num = 2;
        s.axis_ratio_den = 1;
        // After we lock gravity, require smash to align at least ~70% with gravity direction.
        s.axis_bias_min_dot = 0;
        s
    }

    pub fn update(&mut self, now_ms: u64, sample: &ImuSample) -> bool {
        let mag_sq = sample.accel_mag_sq();
        let gyro_sq = sample.gyro_mag_sq();
        let in_cooldown = now_ms.saturating_sub(self.last_trigger_ms) < self.cooldown_ms as u64;

        // Freefall guard: if the previous sample was near zero-g, treat the spike as a drop.
        let freefall_guard = self.last_freefall;
        self.last_freefall = mag_sq < self.freefall_sq;

        let rising_fast = mag_sq.saturating_sub(self.last_mag_sq) >= self.rise_threshold_sq;
        self.last_mag_sq = mag_sq;

        // Learn gravity direction during early samples (first 256 reads) when movement is small.
        if self.gravity_samples < u16::MAX {
            // Simple low-pass: average until we reach 256 samples, but only if mag is near 1g-2g.
            if mag_sq > 600_000 && mag_sq < 4_000_000 {
                let k = (self.gravity_samples as i64).saturating_add(1);
                for i in 0..3 {
                    // incremental average
                    self.gravity_dir[i] = (((self.gravity_dir[i] as i64) * self.gravity_samples as i64
                        + sample.accel[i] as i64)
                        / k) as i32;
                }
                if self.gravity_samples < 256 {
                    self.gravity_samples += 1;
                }
                if self.gravity_samples == 256 && self.axis_bias_min_dot == 0 {
                    // enable axis bias: require at least ~60% alignment with gravity vector
                    let gmag_sq: i64 = self
                        .gravity_dir
                        .iter()
                        .map(|v| {
                            let vv = *v as i64;
                            vv * vv
                        })
                        .sum();
                    // 0.7 * |g|^2 approximate as 0.49 * gmag_sq
                    self.axis_bias_min_dot = (gmag_sq as i128 * 49 / 100) as i64;
                }
            }
        }

        // Axis bias check: dot(sample, gravity_dir) should be large and same direction.
        let mut axis_ok = true;
        if self.axis_bias_min_dot > 0 {
            let dot: i64 = (sample.accel[0] as i64 * self.gravity_dir[0] as i64)
                + (sample.accel[1] as i64 * self.gravity_dir[1] as i64)
                + (sample.accel[2] as i64 * self.gravity_dir[2] as i64);
            axis_ok = dot >= self.axis_bias_min_dot;
        }

        // Baseline magnitude (|a|^2) EMA for shake rejection: only update when gyro is quiet.
        if gyro_sq < 10_000 && mag_sq > 500_000 && mag_sq < 2_500_000 {
            if self.baseline_mag_sq == 0 {
                self.baseline_mag_sq = mag_sq;
            } else {
                // EMA with alpha ~1/16
                self.baseline_mag_sq = ((self.baseline_mag_sq * 15) + mag_sq) / 16;
            }
        }

        // Dominant axis check: max axis at least ratio over others.
        let mut ratio_ok = true;
        if self.axis_ratio_num > 0 {
            let mut axes = [
                sample.accel[0].abs() as i32,
                sample.accel[1].abs() as i32,
                sample.accel[2].abs() as i32,
            ];
            axes.sort_unstable();
            let max = axes[2] as i64;
            let mid = axes[1] as i64;
            let lo = axes[0] as i64;
            let num = self.axis_ratio_num as i64;
            let den = self.axis_ratio_den as i64;
            ratio_ok = max * den >= mid * num && max * den >= lo * num;
        }

        // Gyro check: allow high gyro if accel is very high, otherwise enforce limit.
        let gyro_ok = if mag_sq > self.threshold_sq.saturating_mul(4) {
            true
        } else {
            gyro_sq < self.gyro_limit_sq
        };

        // Require a sharp jump over baseline to reject slow wiggles.
        let mut jump_ok = true;
        if self.baseline_mag_sq > 0 {
            // need mag_sq at least 3x baseline to count as smash
            jump_ok = mag_sq.saturating_mul(1) > self.baseline_mag_sq.saturating_mul(3);
        }

        let hit = !in_cooldown
            && !freefall_guard
            && mag_sq >= self.threshold_sq
            && rising_fast
            && gyro_ok
            && axis_ok
            && ratio_ok
            && jump_ok;

        if hit {
            self.last_trigger_ms = now_ms;
        }

        hit
    }
}
