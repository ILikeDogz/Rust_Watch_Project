// Functions for initializing RTC and handling wake from deep sleep

use crate::drivers::rtc_pcf85063::{datetime_is_valid, datetime_to_unix, Pcf85063};
use crate::hal::bus::SharedI2cRef;
use crate::hal::timer::TimerDelay;
use crate::ui::{clear_all_caches, set_clock_seconds};
use embedded_hal::delay::DelayNs;
use esp_hal::{
    rtc_cntl::{reset_reason, wakeup_cause, Rtc, SocResetReason},
    system::Cpu,
};

pub fn init_rtc_and_wake<'a>(lpwr: esp_hal::peripherals::LPWR<'a>) -> (Rtc<'a>, u64, bool) {
    let rtc = Rtc::new(lpwr);
    let rtc_boot_time_us: u64 = rtc.current_time_us();

    let woke_from_sleep = {
        let reason = reset_reason(Cpu::ProCpu).unwrap_or(SocResetReason::ChipPowerOn);
        let wake = wakeup_cause();

        // Check if waking from deep sleep
        // After deep sleep, the RTC timer continues but everything else resets
        let from_sleep = matches!(reason, SocResetReason::CoreDeepSleep)
            || matches!(
                wake,
                esp_hal::system::SleepSource::Gpio
                    | esp_hal::system::SleepSource::Ext0
                    | esp_hal::system::SleepSource::Ext1
                    | esp_hal::system::SleepSource::Timer
            );

        if from_sleep {
            // RTC kept running during sleep - restore clock from RTC value
            let restored_secs = (rtc_boot_time_us / 1_000_000) as u32;
            set_clock_seconds(restored_secs);
            clear_all_caches();
        }
        from_sleep
    };

    (rtc, rtc_boot_time_us, woke_from_sleep)
}

pub fn read_rtc_seconds(bus: SharedI2cRef) -> Option<u32> {
    let mut delay = TimerDelay;
    delay.delay_ms(20);
    for attempt in 0..3 {
        let rtc_dev = embedded_hal_bus::i2c::RefCellDevice::new(bus);
        let mut rtc_handle = Pcf85063::new(rtc_dev);
        let read = rtc_handle.read_datetime().ok().and_then(|(dt, vl)| {
            if vl {
                None
            } else if datetime_is_valid(&dt) {
                Some(datetime_to_unix(&dt))
            } else {
                None
            }
        });
        if read.is_some() {
            return read;
        }
        delay.delay_ms(10 + attempt * 10);
    }
    None
}
