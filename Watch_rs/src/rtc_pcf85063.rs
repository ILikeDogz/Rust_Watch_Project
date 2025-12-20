// RTC driver for PCF85063A/PCF85063TP real-time clock chips.
// Datasheet: https://files.waveshare.com/wiki/common/Pcf85063atl1118-NdPQpTGE-loeW7GbZ7.pdf

use embedded_hal::i2c::I2c;

#[derive(Copy, Clone, Debug)]
pub struct DateTime {
    pub year: u16,  // full year, e.g., 2024
    pub month: u8,  // 1-12
    pub day: u8,    // 1-31
    pub hour: u8,   // 0-23
    pub minute: u8, // 0-59
    pub second: u8, // 0-59
}

pub struct Pcf85063<I2C> {
    i2c: I2C,
}

impl<I2C, E> Pcf85063<I2C>
where
    I2C: I2c<Error = E>,
{
    pub fn new(i2c: I2C) -> Self {
        Self { i2c }
    }

    pub fn into_inner(self) -> I2C {
        self.i2c
    }

    // Read datetime. Returns (dt, vl_flag) where vl_flag == true means time is unreliable (power loss).
    pub fn read_datetime(&mut self) -> Result<(DateTime, bool), E> {
        let mut buf = [0u8; 7];
        // Time registers start at 0x04: sec, min, hour, day, weekday, month, year
        self.i2c.write_read(0x51, &[0x04], &mut buf)?;
        let vl = (buf[0] & 0x80) != 0;
        let sec = bcd_decode(buf[0] & 0x7F);
        let min = bcd_decode(buf[1] & 0x7F);
        let hour = bcd_decode(buf[2] & 0x3F);
        let day = bcd_decode(buf[3] & 0x3F);
        let month_raw = buf[5];
        let month = bcd_decode(month_raw & 0x1F);
        let year = if (month_raw & 0x80) != 0 {
            1900u16 + bcd_decode(buf[6]) as u16
        } else {
            2000u16 + bcd_decode(buf[6]) as u16
        };
        Ok((
            DateTime {
                year,
                month,
                day,
                hour,
                minute: min,
                second: sec,
            },
            vl,
        ))
    }

    // Set datetime. Ignores weekday field.
    pub fn set_datetime(&mut self, dt: &DateTime) -> Result<(), E> {
        let yr = (dt.year % 100) as u8;
        let data = [
            0x04,
            bcd_encode(dt.second),
            bcd_encode(dt.minute),
            bcd_encode(dt.hour),
            bcd_encode(dt.day),
            0, // weekday not used
            bcd_encode(dt.month),
            bcd_encode(yr),
        ];
        self.i2c.write(0x51, &data)?;
        Ok(())
    }
}

// BCD encode/decode helpers
fn bcd_decode(v: u8) -> u8 {
    (v & 0x0F) + ((v >> 4) * 10)
}

// BCD encode
fn bcd_encode(v: u8) -> u8 {
    ((v / 10) << 4) | (v % 10)
}

// Days since 1970-01-01 for UTC conversion (simple, handles leap years through 2099).
fn days_since_unix(year: u16, month: u8, day: u8) -> u32 {
    let y = year as i32;
    let m = month as i32;
    let d = day as i32;
    let (y1, m1) = if m <= 2 { (y - 1, m + 12) } else { (y, m) };
    let era = y1 / 400;
    let yoe = y1 - era * 400; // year of era
    let doy = (153 * (m1 + 1) / 5 + d - 123) as i32; // days since March 1
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // days since era
    (era * 146097 + doe - 719468) as u32 // 719468 = days from 0000-03-01 to 1970-01-01
}

// Convert DateTime to Unix timestamp (seconds since 1970-01-01).
pub fn datetime_to_unix(dt: &DateTime) -> u32 {
    let days = days_since_unix(dt.year, dt.month, dt.day) as u64;
    let secs = days
        .saturating_mul(86_400) // 86400 seconds in a day
        .saturating_add((dt.hour as u64) * 3600) // 3600 seconds in an hour
        .saturating_add((dt.minute as u64) * 60) // 60 seconds in a minute
        .saturating_add(dt.second as u64); // add seconds
    secs.min(u32::MAX as u64) as u32
}

// Basic sanity check on decoded RTC time.
pub fn datetime_is_valid(dt: &DateTime) -> bool {
    (2020..=2099).contains(&dt.year)
        && (1..=12).contains(&dt.month)
        && (1..=31).contains(&dt.day)
        && dt.hour < 24
        && dt.minute < 60
        && dt.second < 60
}

// Convert Unix timestamp (seconds since 1970-01-01) to DateTime.
pub fn unix_to_datetime(mut ts: u32) -> DateTime {
    let days = ts / 86400;
    ts %= 86400;
    let hour = (ts / 3600) as u8;
    ts %= 3600;
    let minute = (ts / 60) as u8;
    let second = (ts % 60) as u8;

    // Convert days since 1970-01-01 back to date (valid until 2099).
    let z = days as i32 + 719468; // 719468 = days from 0000-03-01 to 1970-01-01
    let era = (z >= 0)
        .then(|| z / 146097) // 146097 = days in 400 years
        .unwrap_or_else(|| (z - 146096) / 146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // year of era, 1460 = days in 4 years, 36524 = days in 100 years, 146096 = days in 400 years
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153; // 153 = days in 5 months
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 }; // March=3,...,January=13,February=14
    let year = y + if month <= 2 { 1 } else { 0 }; // adjust year if month is Jan or Feb

    DateTime {
        year: year as u16,
        month: month as u8,
        day: day as u8,
        hour,
        minute,
        second,
    }
}
