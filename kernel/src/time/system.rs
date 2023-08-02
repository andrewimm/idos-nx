//! Utilities for managing system time

use core::sync::atomic::{AtomicU32, Ordering};

use spin::Mutex;

use super::date::DateTime;

// The system timer ticks at ~100Hz
pub const HUNDRED_NS_PER_TICK: u64 = 100002;
pub const MS_PER_TICK: u32 = (HUNDRED_NS_PER_TICK / 10000) as u32;

/// Stores the number of clock ticks since the kernel began execution. This is
/// used for relative time offsets within various kernel internals.
static SYSTEM_TICKS: AtomicU32 = AtomicU32::new(0);

/// Store a known fixed point in time, sourced from CMOS RTC, a NTP service, or
/// something similar. We use the programmable timer to update an offset
/// relative to this number.
static KNOWN_TIME: Mutex<TimestampHires> = Mutex::new(TimestampHires(0));
/// Store an offset, regularly updated by the programmable timer
static TIME_OFFSET: Mutex<TimestampHires> = Mutex::new(TimestampHires(0));

pub fn tick() {
    SYSTEM_TICKS.fetch_add(1, Ordering::SeqCst);
    increment_offset(HUNDRED_NS_PER_TICK);
}

pub fn get_system_ticks() -> u32 {
    SYSTEM_TICKS.load(Ordering::SeqCst)
}

/// High-resolution 64-bit timestamp representing the number of 100ns
/// increments since midnight 1 January 1980
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct TimestampHires(pub u64);

impl TimestampHires {
    pub fn set(&mut self, value: u64) {
        self.0 = value;
    }

    pub fn increment(&mut self, value: u64) {
        self.0 += value;
    }

    pub fn in_ms(&self) -> u64 {
        self.0 / 10_000
    }

    pub fn in_seconds(&self) -> u64 {
        self.0 / 10_000_000
    }

    pub fn from_timestamp(ts: Timestamp) -> Self {
        Self(ts.0 as u64 * 10_000_000)
    }

    pub fn to_timestamp(&self) -> Timestamp {
        Timestamp(self.in_seconds() as u32)
    }
}

impl core::ops::Add for TimestampHires {
    type Output = TimestampHires;

    fn add(self, rhs: Self) -> Self::Output {
        TimestampHires(self.0 + rhs.0)
    }
}

/// Unsigned, 32-bit number representing the number of seconds passed since
/// midnight on 1 January 1980. It neglects leap seconds.
/// This is NOT the same as POSIX time!
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Timestamp(pub u32);

impl Timestamp {
    pub fn to_datetime(&self) -> DateTime {
        DateTime::from_timestamp(*self)
    }

    pub fn total_minutes(&self) -> u32 {
        self.0 / 60
    }

    pub fn as_u32(&self) -> u32 {
        self.0
    }
}

/// Reset the reference point time
pub fn reset_known_time(time: u64) {
    // TODO: mark this as critical, not to be interrupted
    KNOWN_TIME.lock().set(time);
    TIME_OFFSET.lock().set(0);
}

pub fn get_system_time() -> TimestampHires {
    // TODO: mark this as critical, not to be interrupted
    let known = *KNOWN_TIME.lock();
    let offset = *TIME_OFFSET.lock();

    known + offset
}

pub fn get_offset_seconds() -> u64 {
    let offset = *TIME_OFFSET.lock();

    offset.in_seconds()
}

pub fn increment_offset(delta: u64) {
    TIME_OFFSET.lock().increment(delta);
}

pub fn initialize_time_from_rtc() {
    let cmos_time = crate::hardware::rtc::read_rtc_time();
    let dt = cmos_time.to_datetime();
    let timestamp = cmos_time.to_datetime().to_timestamp();
    let system_time = TimestampHires::from_timestamp(timestamp);
    reset_known_time(system_time.0)
}

