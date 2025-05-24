#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(C, packed)]
pub struct Date {
    pub day: u8,
    pub month: u8,
    pub year: u16,
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(C, packed)]
pub struct Time {
    pub seconds: u8,
    pub minutes: u8,
    pub hours: u8,
}

pub struct DateTime {
    pub date: Date,
    pub time: Time,
}

pub const SECONDS_IN_DAY: u32 = 60 * 60 * 24;

const MONTH_START_OFFSET: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];

pub fn year_offset_from_days(days: u32) -> u32 {
    let hundredths = days * 100;
    hundredths / 36525
}

impl DateTime {
    pub fn from_timestamp(ts: u32) -> Self {
        let days = ts / SECONDS_IN_DAY;
        let raw_time = ts % SECONDS_IN_DAY;
        let year_offset = year_offset_from_days(days);
        let quadrennial_days = days % (365 + 365 + 365 + 366);
        let year_days = if quadrennial_days > 365 {
            (quadrennial_days - 366) % 365
        } else {
            quadrennial_days
        };
        let mut month = 0;
        let mut leap = 0;
        while month < 12 && MONTH_START_OFFSET[month] + leap <= year_days {
            month += 1;
            if month == 2 && year_offset % 4 == 0 {
                // 2000 is a leap year, don't need to check 2100
                leap = 1;
            }
        }
        let mut day = year_days + 1 - MONTH_START_OFFSET[month - 1];
        if month > 2 {
            day -= leap;
        }

        let total_minutes = raw_time / 60;
        let seconds = raw_time % 60;
        let hours = total_minutes / 60;
        let minutes = total_minutes % 60;

        DateTime {
            date: Date {
                day: day as u8,
                month: month as u8,
                year: year_offset as u16 + 1980,
            },

            time: Time {
                seconds: seconds as u8,
                minutes: minutes as u8,
                hours: hours as u8,
            },
        }
    }
}
