use std::time::SystemTime;

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveTime, Timelike};

#[derive(Debug, Clone, Copy)]
pub struct Date {
    repr: u16,
}

impl Date {
    pub fn new(repr: u16) -> anyhow::Result<Date> {
        let date = Date { repr };

        anyhow::ensure!(date.day() <= 31, "invalid day for date: {} (0x{:#X})", date.day(), repr);
        anyhow::ensure!(
            date.month() <= 12,
            "invalid month for date: {} (0x{:#X})",
            date.month(),
            repr
        );

        Ok(date)
    }

    fn from_day_month_year(day: u8, month: u8, year: u16) -> anyhow::Result<Date> {
        anyhow::ensure!(day <= 31, "invalid day: {}", day);
        anyhow::ensure!(month <= 12, "invalid month: {}", month);
        anyhow::ensure!(1980 <= year && year <= 2107, "invalid year: {}", year);

        let repr = day as u16 | (month as u16) << 4 | (year - 1980) << 8;

        Ok(Date { repr })
    }

    pub fn from_datetime(datetime: DateTime<Local>) -> anyhow::Result<Date> {
        let date = datetime.date_naive();

        Date::from_day_month_year(
            date.day() as u8,
            date.month0() as u8 + 1,
            date.year_ce().1 as u16,
        )
    }

    pub fn repr(&self) -> u16 {
        self.repr
    }

    pub fn day(&self) -> u8 {
        (self.repr & 0x1F) as u8
    }

    pub fn month(&self) -> u8 {
        ((self.repr & 0x1E0) >> 5) as u8
    }

    pub fn year(&self) -> u16 {
        ((self.repr & 0xFE00) >> 9) as u16 + 1980
    }

    pub fn to_naive_date(&self) -> NaiveDate {
        NaiveDate::from_ymd_opt(self.year() as i32, self.month() as u32, self.day() as u32).unwrap()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Time {
    repr: u16,
}

impl Time {
    pub fn new(time: u16) -> anyhow::Result<Time> {
        let time = Time { repr: time };

        Ok(time)
    }

    fn from_seconds_minutes_hours(seconds: u8, minutes: u8, hours: u8) -> anyhow::Result<Time> {
        anyhow::ensure!(seconds <= 58 && seconds % 2 == 0, "invalid seconds: {}", seconds);
        anyhow::ensure!(minutes <= 59, "invalid minutes: {}", minutes);
        anyhow::ensure!(hours <= 23, "invalid hours: {}", hours);

        let repr = (seconds >> 1) as u16 | (minutes as u16) << 5 | (hours as u16) << 11;

        Ok(Time { repr })
    }

    pub fn from_datetime(datetime: DateTime<Local>) -> anyhow::Result<Time> {
        let time = datetime.time();

        let seconds = (time.second() as u8) & !0x01;

        Time::from_seconds_minutes_hours(seconds, time.minute() as u8, time.hour() as u8)
    }

    pub fn repr(&self) -> u16 {
        self.repr
    }

    pub fn second(&self) -> u8 {
        2 * (self.repr & 0x1F) as u8
    }

    pub fn minute(&self) -> u8 {
        ((self.repr >> 5) & 0x3F) as u8
    }

    pub fn hour(&self) -> u8 {
        ((self.repr >> 11) & 0x1F) as u8
    }

    pub fn to_naive_time(&self) -> NaiveTime {
        NaiveTime::from_hms_opt(self.hour() as u32, self.minute() as u32, self.second() as u32)
            .unwrap()
    }
}
