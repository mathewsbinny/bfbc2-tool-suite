//! The local time as Python's `datetime.now()` / `date.today()` give it to the tools' logs.

/// A local date and time, to the millisecond.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Local {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
    pub milli: u16,
}

impl Local {
    /// `date.isoformat()`: `YYYY-MM-DD`.
    pub fn date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// `now.strftime('%Y-%m-%d %H:%M:%S,') + f'{now.microsecond // 1000:03d}'`: the logs' stamp.
    pub fn stamp(&self) -> String {
        format!(
            "{} {:02}:{:02}:{:02},{:03}",
            self.date(),
            self.hour,
            self.minute,
            self.second,
            self.milli
        )
    }
}

/// Now, in the local time zone (Windows: `GetLocalTime`, what the C runtime's `localtime` that Python calls gives).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn now() -> Local {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut t = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    // SAFETY: GetLocalTime only writes the SYSTEMTIME it is given, which lives for the call.
    unsafe { GetLocalTime(&raw mut t) };
    Local {
        year: t.wYear,
        month: t.wMonth,
        day: t.wDay,
        hour: t.wHour,
        minute: t.wMinute,
        second: t.wSecond,
        milli: t.wMilliseconds,
    }
}

/// `time.localtime(t)` (the seconds floored, as Python takes a float): the local date and time of a moment given as
/// seconds since 1970 (Windows: `SystemTimeToTzSpecificLocalTime` with the current time zone, as the C runtime's
/// `localtime`). Before 1970 or unconvertible: 1980-01-01 (the archive's earliest time).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn local_of(t: f64) -> Local {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
    let fallback = Local {
        year: 1980,
        month: 1,
        day: 1,
        hour: 0,
        minute: 0,
        second: 0,
        milli: 0,
    };
    if !(0.0..1e12).contains(&t) {
        return fallback;
    }
    let ticks = (t.floor() as u64 + 11_644_473_600) * 10_000_000;
    let ft = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    let zero = SYSTEMTIME {
        wYear: 0,
        wMonth: 0,
        wDayOfWeek: 0,
        wDay: 0,
        wHour: 0,
        wMinute: 0,
        wSecond: 0,
        wMilliseconds: 0,
    };
    let (mut utc, mut local) = (zero, zero);
    // SAFETY: both calls only read and write the structures they are given, which live for the calls.
    let ok = unsafe {
        FileTimeToSystemTime(&raw const ft, &raw mut utc) != 0
            && SystemTimeToTzSpecificLocalTime(std::ptr::null(), &raw const utc, &raw mut local) != 0
    };
    if !ok {
        return fallback;
    }
    Local {
        year: local.wYear,
        month: local.wMonth,
        day: local.wDay,
        hour: local.wHour,
        minute: local.wMinute,
        second: local.wSecond,
        milli: 0,
    }
}

/// `time.localtime(t)` elsewhere than Windows: UTC.
#[cfg(not(windows))]
pub fn local_of(t: f64) -> Local {
    civil(std::time::Duration::from_secs_f64(t.max(0.0)))
}

/// Now, in UTC (elsewhere than Windows, which the suite does not run on: the logs' stamps only).
#[cfg(not(windows))]
pub fn now() -> Local {
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    civil(d)
}

#[cfg(not(windows))]
fn civil(d: std::time::Duration) -> Local {
    let secs = d.as_secs();
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // the civil date of a day count (Howard Hinnant's days_from_civil, inverted)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    Local {
        year: year as u16,
        month: month as u16,
        day: day as u16,
        hour: (rem / 3600) as u16,
        minute: (rem / 60 % 60) as u16,
        second: (rem % 60) as u16,
        milli: d.subsec_millis() as u16,
    }
}
