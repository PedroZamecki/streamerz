//! .NET-style formatting, as in `%tipAmount:c2%` and `%date:dddd, dd MMMM yyyy%`, in the OS locale;
//! and the local time. Numbers are rounded like .NET Framework: to 15 significant digits first, then
//! half away from zero.

use std::sync::OnceLock;

/// What formatting needs from the OS locale. Patterns are .NET custom date/time patterns.
pub struct Locale {
    dec: String,
    group: String,
    currency: String,
    currency_after: bool,
    currency_space: bool,
    /// The date separator, for "/" in patterns.
    date_sep: String,
    months: [String; 12],
    months_abbr: [String; 12],
    /// Sunday first.
    days: [String; 7],
    days_abbr: [String; 7],
    am: String,
    pm: String,
    short_date: String,
    long_date: String,
    long_time: String,
    short_time: String,
    month_day: String,
}

impl Locale {
    /// en-US, as .NET has it.
    fn en_us() -> Self {
        let s = |a: &[&str]| a.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        let months = s(&[
            "January",
            "February",
            "March",
            "April",
            "May",
            "June",
            "July",
            "August",
            "September",
            "October",
            "November",
            "December",
        ]);
        let days = s(&[
            "Sunday",
            "Monday",
            "Tuesday",
            "Wednesday",
            "Thursday",
            "Friday",
            "Saturday",
        ]);
        Locale {
            dec: ".".into(),
            group: ",".into(),
            currency: "$".into(),
            currency_after: false,
            currency_space: false,
            date_sep: "/".into(),
            months_abbr: std::array::from_fn(|i| months[i][..3].to_string()),
            months: std::array::from_fn(|i| months[i].clone()),
            days_abbr: std::array::from_fn(|i| days[i][..3].to_string()),
            days: std::array::from_fn(|i| days[i].clone()),
            am: "AM".into(),
            pm: "PM".into(),
            short_date: "M/d/yyyy".into(),
            long_date: "dddd, MMMM d, yyyy".into(),
            long_time: "h:mm:ss tt".into(),
            short_time: "h:mm tt".into(),
            month_day: "MMMM d".into(),
        }
    }

    /// Long date and "month day" patterns for a short date's day/month order (glibc has neither).
    fn set_order(&mut self) {
        let d = self.short_date.find('d').unwrap_or(0);
        let m = self.short_date.find('M').unwrap_or(0);
        let y = self.short_date.find('y').unwrap_or(0);
        (self.long_date, self.month_day) = if y < d && y < m {
            ("yyyy MMMM d, dddd".into(), "MMMM d".into())
        } else if d < m {
            ("dddd, d MMMM yyyy".into(), "d MMMM".into())
        } else {
            ("dddd, MMMM d, yyyy".into(), "MMMM d".into())
        };
    }
}

/// How the window shows dates and times (.NET patterns); empty follows the system.
static SHOWN_AS: std::sync::RwLock<(String, String)> = std::sync::RwLock::new((String::new(), String::new()));

/// The date and time patterns the user picked in Settings ("" for the system's).
pub fn set_shown_as(date: &str, time: &str) {
    if let Ok(mut f) = SHOWN_AS.write() {
        *f = (date.to_string(), time.to_string());
    }
}

/// The patterns the window uses now: the picked ones, else the system's.
pub fn shown_patterns() -> (String, String) {
    let (d, t) = SHOWN_AS.read().map(|f| f.clone()).unwrap_or_default();
    let l = locale();
    // A picked "/" is a slash, not the system's separator.
    let d = if d.is_empty() {
        l.short_date.clone()
    } else {
        literal_slashes(&d)
    };
    (d, if t.is_empty() { l.short_time.clone() } else { t })
}

/// A pattern with each `/` outside quotes and escapes written as a plain slash (`\/`).
fn literal_slashes(p: &str) -> String {
    let mut out = String::with_capacity(p.len() + 4);
    let (mut quote, mut escaped) = (None, false);
    for c in p.chars() {
        match c {
            _ if escaped => escaped = false,
            '\\' if quote.is_none() => escaped = true,
            '\'' | '"' if quote.is_none() => quote = Some(c),
            _ if Some(c) == quote => quote = None,
            '/' if quote.is_none() => out.push('\\'),
            _ => {}
        }
        out.push(c);
    }
    out
}

/// The system's short date and time patterns.
pub fn locale_patterns() -> (String, String) {
    let l = locale();
    (l.short_date.clone(), l.short_time.clone())
}

/// `t`'s date as the user reads dates.
pub fn shown_date(t: &Time) -> String {
    date(t, &shown_patterns().0)
}

/// `t`'s time of day as the user reads times.
pub fn shown_time(t: &Time) -> String {
    date(t, &shown_patterns().1)
}

/// An ISO 8601 time ("2024-08-17T12:34:56-03:00") as the user reads it, in local time: the date, and
/// the time after it with `time`. Empty when it isn't one.
pub fn shown_iso(iso: &str, time: bool) -> String {
    let Some(secs) = crate::engine::rest_parse_iso(iso).filter(|_| !iso.starts_with("0001-01-01")) else {
        return String::new();
    };
    let t = local_time(secs);
    if time {
        format!("{} {}", shown_date(&t), shown_time(&t))
    } else {
        shown_date(&t)
    }
}

/// Unix seconds as the user reads a date and a time.
pub fn shown_when(secs: i64) -> String {
    let t = local_time(secs);
    format!("{} {}", shown_date(&t), shown_time(&t))
}

/// Unix seconds as the user reads a time of day (chat lines).
pub fn shown_clock(secs: i64) -> String {
    shown_time(&local_time(secs))
}

/// Unix seconds as a local date and time, with the offset in effect then (not now's: a time from
/// before a daylight saving change keeps its own hour).
pub fn local_time(secs: i64) -> Time {
    let offset = offset_at(secs);
    Time {
        offset,
        ..crate::engine::civil_time(secs + i64::from(offset))
    }
}

/// Unix seconds of a local date and time, given as seconds since 1970 read as if it were UTC, with the
/// offset in effect at that moment (not now's).
pub fn from_local(naive: i64) -> i64 {
    from_local_with(naive, offset_at)
}

fn from_local_with(naive: i64, offset_at: impl Fn(i64) -> i32) -> i64 {
    let guess = naive - i64::from(offset_at(naive));
    naive - i64::from(offset_at(guess))
}

pub fn locale() -> &'static Locale {
    static L: OnceLock<Locale> = OnceLock::new();
    // Tests read the same everywhere: a machine with a decimal comma (pt-BR, de-DE) once failed them.
    L.get_or_init(if cfg!(test) { Locale::en_us } else { os_locale })
}

/// Parse an ungrouped number formatted in the user's locale.
pub fn parse_number(text: &str) -> Option<f64> {
    parse_number_with_decimal(text, &locale().dec)
}

fn parse_number_with_decimal(text: &str, decimal: &str) -> Option<f64> {
    let text = text.trim();
    if decimal != "." && text.contains(decimal) {
        if text.contains('.') {
            return None;
        }
        text.replace(decimal, ".").parse().ok()
    } else {
        text.parse().ok()
    }
}

/// The user's locale, through glibc (the same data `date` and `printf` use).
#[cfg(target_os = "linux")]
fn os_locale() -> Locale {
    use std::ffi::{c_char, c_int, c_void, CStr};
    extern "C" {
        fn newlocale(mask: c_int, locale: *const c_char, base: *mut c_void) -> *mut c_void;
        fn nl_langinfo_l(item: c_int, l: *mut c_void) -> *const c_char;
    }
    let mut out = Locale::en_us();
    // SAFETY: "" is NUL-terminated; newlocale returns null on failure, checked.
    let l = unsafe { newlocale(8127, c"".as_ptr(), std::ptr::null_mut()) }; // LC_ALL_MASK
    if l.is_null() {
        return out;
    }
    let get = |item: c_int| -> String {
        // SAFETY: valid locale and item; glibc returns a NUL-terminated string (maybe empty).
        unsafe { CStr::from_ptr(nl_langinfo_l(item, l)) }
            .to_string_lossy()
            .into_owned()
    };
    keep(&mut out.dec, get(0x10000));
    out.group = get(0x10001);
    // CRNCYSTR: "-" before the number, "+" after, "." in place of the decimal point.
    let c = get(0x4000f);
    if let Some(sym) = c.get(1..).filter(|s| !s.is_empty()) {
        out.currency = sym.into();
        out.currency_after = c.starts_with('+');
        out.currency_space = get(0x4000a).as_bytes().first().is_some_and(|&b| b == 1);
    }
    for i in 0..12 {
        out.months[i] = get(0x2001a + i as c_int);
        out.months_abbr[i] = get(0x2000e + i as c_int);
    }
    for i in 0..7 {
        out.days[i] = get(0x20007 + i as c_int);
        out.days_abbr[i] = get(0x20000 + i as c_int);
    }
    out.am = get(0x20026);
    out.pm = get(0x20027);
    out.short_date = strftime_to_net(&get(0x20029));
    out.date_sep = date_separator(&out.short_date);
    out.short_date = out.short_date.replace(&out.date_sep, "/");
    out.long_time = strftime_to_net(&get(0x2002a));
    out.short_time = out.long_time.replace(":ss", "").replace(".ss", "");
    out.set_order();
    out
}

/// The user's locale, from Windows (where .NET gets it too).
#[cfg(windows)]
fn os_locale() -> Locale {
    use windows_sys::Win32::Globalization::GetLocaleInfoEx;
    let mut out = Locale::en_us();
    let get = |t: u32| -> String {
        let mut buf = [0u16; 128];
        // SAFETY: null is LOCALE_NAME_USER_DEFAULT; the buffer and its length match.
        let n = unsafe { GetLocaleInfoEx(std::ptr::null(), t, buf.as_mut_ptr(), buf.len() as i32) };
        String::from_utf16_lossy(&buf[..(n.max(1) - 1) as usize])
    };
    keep(&mut out.dec, get(0x0E));
    out.group = get(0x0F);
    keep(&mut out.currency, get(0x14));
    let pos = get(0x1B); // 0 "$1", 1 "1$", 2 "$ 1", 3 "1 $"
    out.currency_after = pos == "1" || pos == "3";
    out.currency_space = pos == "2" || pos == "3";
    keep(&mut out.date_sep, get(0x1D));
    for i in 0..12 {
        keep(&mut out.months[i], get(0x38 + i as u32));
        keep(&mut out.months_abbr[i], get(0x44 + i as u32));
    }
    // Windows starts its day names on Monday.
    for i in 0..7 {
        keep(&mut out.days[(i + 1) % 7], get(0x2A + i as u32));
        keep(&mut out.days_abbr[(i + 1) % 7], get(0x31 + i as u32));
    }
    // 24 hour locales may have no AM and PM: kept empty, as .NET has them.
    out.am = get(0x28);
    out.pm = get(0x29);
    keep(&mut out.short_date, get(0x1F));
    keep(&mut out.long_time, get(0x1003));
    keep(&mut out.short_time, get(0x79));
    out.set_order();
    keep(&mut out.long_date, get(0x20));
    out
}

/// Sets `field` to what the OS said, unless it said nothing (a failed call): the default stays.
#[cfg_attr(not(any(target_os = "linux", windows)), allow(dead_code))]
fn keep(field: &mut String, value: String) {
    if !value.is_empty() {
        *field = value;
    }
}

/// The date separator of a short date pattern: its first character that isn't a letter, a digit
/// or an escape ("/" when there is none).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn date_separator(pattern: &str) -> String {
    pattern
        .chars()
        .find(|c| !c.is_alphanumeric() && !matches!(c, '\'' | '\\'))
        .map_or("/".into(), String::from)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn os_locale() -> Locale {
    Locale::en_us()
}

/// A strftime pattern (glibc's D_FMT/T_FMT) as a .NET one. Its other letters are quoted, as .NET
/// would read them as fields ("%Hh%M" is "HH'h'mm").
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn strftime_to_net(f: &str) -> String {
    let mut out = String::new();
    let mut it = f.chars().peekable();
    while let Some(c) = it.next() {
        if c != '%' {
            if c.is_ascii_alphabetic() || matches!(c, '\'' | '"' | '\\' | '%') {
                out.push('\\');
            }
            out.push(c);
            continue;
        }
        // The E and O modifiers (other eras and digits) change nothing here.
        while it.next_if(|&m| m == 'E' || m == 'O').is_some() {}
        out += match it.next() {
            Some('d') => "dd",
            Some('e') => "d",
            Some('m') => "MM",
            Some('Y') => "yyyy",
            Some('y') => "yy",
            Some('H') => "HH",
            Some('k') => "H",
            Some('I') => "hh",
            Some('l') => "h",
            Some('M') => "mm",
            Some('S') => "ss",
            Some('p') | Some('P') => "tt",
            Some('r') => "hh:mm:ss tt",
            Some('T') => "HH:mm:ss",
            Some('R') => "HH:mm",
            Some('D') => "MM/dd/yy",
            Some('F') => "yyyy-MM-dd",
            Some('b') | Some('h') => "MMM",
            Some('B') => "MMMM",
            Some('a') => "ddd",
            Some('A') => "dddd",
            Some('%') => "\\%",
            Some('n') | Some('t') => " ",
            _ => "",
        };
    }
    out.trim().to_string()
}

/// A local date and time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Time {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub min: u32,
    pub sec: u32,
    pub nanos: u32,
    /// 0 = Sunday.
    pub weekday: u32,
    /// Offset from UTC, in seconds.
    pub offset: i32,
}

/// `bytes` as lowercase hex, two digits each (a digest's usual text).
pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX_LOWER[usize::from(b >> 4)] as char);
        out.push(HEX_LOWER[usize::from(b & 15)] as char);
    }
    out
}

const HEX_LOWER: &[u8; 16] = b"0123456789abcdef";

pub fn unix_now() -> std::time::Duration {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
}

/// The C library's `struct tm` (64-bit glibc and macOS).
#[cfg(unix)]
#[repr(C)]
struct Tm {
    sec: i32,
    min: i32,
    hour: i32,
    mday: i32,
    mon: i32,
    year: i32,
    wday: i32,
    yday: i32,
    isdst: i32,
    gmtoff: i64,
    zone: *const u8,
}

/// `secs` in the local time zone, by the C library.
#[cfg(unix)]
fn local_tm(secs: i64) -> Tm {
    extern "C" {
        fn localtime_r(t: *const i64, tm: *mut Tm) -> *mut Tm;
    }
    // SAFETY: Tm matches glibc's struct tm on 64-bit; zeroed is a valid value for it, and stays
    // zeroed (UTC) when localtime_r fails.
    let mut tm: Tm = unsafe { std::mem::zeroed() };
    unsafe { localtime_r(&secs, &mut tm) };
    tm
}

/// The local offset from UTC at Unix time `secs`, in seconds (daylight saving time included).
#[cfg(unix)]
pub fn offset_at(secs: i64) -> i32 {
    local_tm(secs).gmtoff as i32
}

/// Now, in the local time zone.
#[cfg(unix)]
pub fn now() -> Time {
    let d = unix_now();
    let tm = local_tm(d.as_secs() as i64);
    Time {
        year: tm.year + 1900,
        month: tm.mon as u32 + 1,
        day: tm.mday as u32,
        hour: tm.hour as u32,
        min: tm.min as u32,
        sec: tm.sec as u32,
        nanos: d.subsec_nanos(),
        weekday: tm.wday as u32,
        offset: tm.gmtoff as i32,
    }
}

/// Days since 1970-01-01 of a civil date (Howard Hinnant's algorithm).
#[cfg_attr(not(windows), allow(dead_code))]
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let (m, d) = (i64::from(m), i64::from(d));
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468
}

/// The local offset from UTC at Unix time `secs`, in seconds (daylight saving time included).
#[cfg(windows)]
pub fn offset_at(secs: i64) -> i32 {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::Time::SystemTimeToTzSpecificLocalTime;
    let u = crate::engine::civil_time(secs);
    if !(1601..=30827).contains(&u.year) {
        return now().offset;
    }
    let utc = SYSTEMTIME {
        wYear: u.year as u16,
        wMonth: u.month as u16,
        wDayOfWeek: u.weekday as u16,
        wDay: u.day as u16,
        wHour: u.hour as u16,
        wMinute: u.min as u16,
        wSecond: u.sec as u16,
        wMilliseconds: 0,
    };
    // SAFETY: plain in and out structs; null is the time zone in use.
    let mut l: SYSTEMTIME = unsafe { std::mem::zeroed() };
    if unsafe { SystemTimeToTzSpecificLocalTime(std::ptr::null(), &utc, &mut l) } == 0 {
        return now().offset;
    }
    let local = days_from_civil(i64::from(l.wYear), u32::from(l.wMonth), u32::from(l.wDay)) * 86_400
        + i64::from(l.wHour) * 3600
        + i64::from(l.wMinute) * 60
        + i64::from(l.wSecond);
    (local - secs) as i32
}

#[cfg(windows)]
pub fn now() -> Time {
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    // SAFETY: plain out-structs, zeroed is valid for both.
    let (st, tz, dst) = unsafe {
        let mut st = std::mem::zeroed();
        GetLocalTime(&mut st);
        let mut tz: TIME_ZONE_INFORMATION = std::mem::zeroed();
        let dst = GetTimeZoneInformation(&mut tz);
        (st, tz, dst)
    };
    // Bias is minutes west of UTC; 2 = daylight time in effect.
    let bias = tz.Bias + if dst == 2 { tz.DaylightBias } else { tz.StandardBias };
    Time {
        year: st.wYear as i32,
        month: st.wMonth as u32,
        day: st.wDay as u32,
        hour: st.wHour as u32,
        min: st.wMinute as u32,
        sec: st.wSecond as u32,
        nanos: st.wMilliseconds as u32 * 1_000_000,
        weekday: st.wDayOfWeek as u32,
        offset: -bias * 60,
    }
}

/// A number's decimal digits: value = 0.d1d2d3... × 10^exp.
struct Digits {
    neg: bool,
    digits: Vec<u8>,
    exp: i32,
}

impl Digits {
    /// An integer exactly; a double to 15 significant digits, like .NET Framework.
    fn of(n: Num) -> Self {
        let (neg, s) = match n {
            Num::Int(i) => (i < 0, format!("{:e}", i.unsigned_abs())),
            Num::Float(f) => (f.is_sign_negative() && f != 0.0, format!("{:.14e}", f.abs())),
        };
        let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
        let mut digits: Vec<u8> = m.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
        while digits.len() > 1 && digits.last() == Some(&0) {
            digits.pop();
        }
        let exp = e.parse::<i32>().unwrap_or(0) + 1;
        if digits == [0] {
            return Digits {
                neg: false,
                digits: vec![],
                exp: 0,
            };
        }
        Digits { neg, digits, exp }
    }

    /// Round half away from zero, keeping `n` digits after the decimal point.
    fn round_frac(&mut self, n: i32) {
        self.round_at(self.exp + n);
    }

    /// Round half away from zero to `keep` significant digits.
    fn round_at(&mut self, keep: i32) {
        if keep < 0 {
            self.digits.clear();
            return;
        }
        let keep = keep as usize;
        if self.digits.len() <= keep {
            return;
        }
        let up = self.digits[keep] >= 5;
        self.digits.truncate(keep);
        if up {
            let mut i = keep;
            loop {
                if i == 0 {
                    self.digits.insert(0, 1);
                    self.exp += 1;
                    break;
                }
                i -= 1;
                if self.digits[i] == 9 {
                    self.digits[i] = 0;
                } else {
                    self.digits[i] += 1;
                    break;
                }
            }
        }
        while self.digits.last() == Some(&0) {
            self.digits.pop();
        }
        if self.digits.is_empty() {
            self.neg = false;
        }
    }

    /// The integer part's digits ("0" for none).
    fn int_part(&self) -> String {
        // Zero scaled up (0 as a percent) is still "0", not "00".
        if self.exp <= 0 || self.digits.is_empty() {
            return "0".into();
        }
        (0..self.exp as usize)
            .map(|i| (b'0' + self.digits.get(i).copied().unwrap_or(0)) as char)
            .collect()
    }

    /// `n` digits after the decimal point.
    fn frac_part(&self, n: usize) -> String {
        (0..n as i32)
            .map(|i| {
                let at = self.exp + i;
                let d = if at < 0 {
                    0
                } else {
                    self.digits.get(at as usize).copied().unwrap_or(0)
                };
                (b'0' + d) as char
            })
            .collect()
    }

    fn is_zero(&self) -> bool {
        self.digits.is_empty()
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Num {
    Int(i64),
    Float(f64),
}

fn group(int: &str, sep: &str) -> String {
    let mut out = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i).is_multiple_of(3) {
            out += sep;
        }
        out.push(c);
    }
    out
}

/// A double as .NET Framework's ToString() writes it: 15 significant digits, exponent when large or
/// tiny (1E+20, 1E-05).
pub fn general(f: f64) -> String {
    if f.is_nan() {
        return "NaN".into();
    }
    if f.is_infinite() {
        return if f > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    g(&Digits::of(Num::Float(f)), 15, 'E', locale())
}

fn g(d: &Digits, precision: i32, e: char, l: &Locale) -> String {
    let mut d = Digits {
        neg: d.neg,
        digits: d.digits.clone(),
        exp: d.exp,
    };
    d.round_at(precision);
    if d.is_zero() {
        return "0".into();
    }
    let sign = if d.neg { "-" } else { "" };
    let x = d.exp - 1;
    if x >= precision || x < -4 {
        let m: String = d.digits.iter().map(|&b| (b'0' + b) as char).collect();
        let m = if m.len() > 1 {
            format!("{}{}{}", &m[..1], l.dec, &m[1..])
        } else {
            m
        };
        return format!("{sign}{m}{e}{}{:02}", if x < 0 { '-' } else { '+' }, x.abs());
    }
    let frac = (d.digits.len() as i32 - d.exp).max(0) as usize;
    let f = d.frac_part(frac);
    let dot = if f.is_empty() { "" } else { &l.dec };
    format!("{sign}{}{dot}{f}", d.int_part())
}

/// A number with a .NET format string (standard like "N2", or custom like "#,##0.00"). None: the
/// format doesn't apply (e.g. "D" or "X" on a fraction), like .NET's FormatException.
pub fn number(n: Num, fmt: &str) -> Option<String> {
    let l = locale();
    let b = fmt.as_bytes();
    let standard = !b.is_empty() && b[0].is_ascii_alphabetic() && b.len() <= 3 && b[1..].iter().all(u8::is_ascii_digit);
    if let Num::Float(f) = n {
        if !f.is_finite() {
            // .NET Framework writes these the same in every format (D and X take no fraction).
            let integral = standard && matches!(b[0], b'D' | b'd' | b'X' | b'x');
            return (!integral).then(|| general(f));
        }
    }
    if !standard {
        return Some(custom_number(n, fmt, l));
    }
    let prec: Option<i32> = fmt[1..].parse().ok();
    let mut d = Digits::of(n);
    let sign = |d: &Digits| if d.neg { "-" } else { "" };
    let fixed = |d: &mut Digits, p: i32, grouped: bool| {
        d.round_frac(p);
        let int = d.int_part();
        let int = if grouped { group(&int, &l.group) } else { int };
        let f = d.frac_part(p as usize);
        if f.is_empty() {
            int
        } else {
            format!("{int}{}{f}", l.dec)
        }
    };
    let int = match n {
        Num::Int(i) => Some(i),
        Num::Float(_) => None,
    };
    Some(match b[0] {
        b'C' | b'c' => {
            let v = fixed(&mut d, prec.unwrap_or(2), true);
            let sp = if l.currency_space { " " } else { "" };
            let v = if l.currency_after {
                format!("{v}{sp}{}", l.currency)
            } else {
                format!("{}{sp}{v}", l.currency)
            };
            format!("{}{v}", sign(&d))
        }
        b'D' | b'd' => {
            let i = int?;
            let s = i.unsigned_abs().to_string();
            let w = prec.unwrap_or(0) as usize;
            format!("{}{s:0>w$}", if i < 0 { "-" } else { "" })
        }
        b'E' | b'e' => {
            let p = prec.unwrap_or(6);
            d.round_at(p + 1);
            let x = if d.is_zero() { 0 } else { d.exp - 1 };
            let m: String = (0..=p as usize)
                .map(|i| (b'0' + d.digits.get(i).copied().unwrap_or(0)) as char)
                .collect();
            let m = if p > 0 {
                format!("{}{}{}", &m[..1], l.dec, &m[1..])
            } else {
                m
            };
            let e = if b[0] == b'E' { 'E' } else { 'e' };
            format!("{}{m}{e}{}{:03}", sign(&d), if x < 0 { '-' } else { '+' }, x.abs())
        }
        b'F' | b'f' => {
            let v = fixed(&mut d, prec.unwrap_or(2), false);
            format!("{}{v}", sign(&d))
        }
        b'N' | b'n' => {
            let v = fixed(&mut d, prec.unwrap_or(2), true);
            format!("{}{v}", sign(&d))
        }
        b'P' | b'p' => {
            d.exp += 2;
            let v = fixed(&mut d, prec.unwrap_or(2), true);
            format!("{}{v}%", sign(&d))
        }
        b'G' | b'g' => {
            let p = match (prec, int) {
                (Some(p), _) if p > 0 => p,
                (_, Some(_)) => 19,
                _ => 15,
            };
            g(&d, p, if b[0] == b'G' { 'E' } else { 'e' }, l)
        }
        b'R' | b'r' => match n {
            Num::Int(i) => i.to_string(),
            Num::Float(f) => {
                // Shortest round-trip digits, written the .NET way.
                let s = format!("{:e}", f.abs());
                let (m, e) = s.split_once('e').unwrap_or((&s, "0"));
                let dg: Vec<u8> = m.bytes().filter(u8::is_ascii_digit).map(|b| b - b'0').collect();
                let d = Digits {
                    neg: f < 0.0,
                    digits: dg,
                    exp: e.parse::<i32>().unwrap_or(0) + 1,
                };
                g(&d, 17, 'E', l)
            }
        },
        b'X' | b'x' => {
            let i = int?;
            let w = prec.unwrap_or(0) as usize;
            if b[0] == b'X' {
                format!("{i:0>w$X}")
            } else {
                format!("{i:0>w$x}")
            }
        }
        _ => return Some(custom_number(n, fmt, l)),
    })
}

/// A custom numeric format: `0` `#` `.` `,` `%` `‰`, quotes and `\`, and `;` sections
/// (positive;negative;zero). ponytail: no exponent (`E0`) in custom formats; nobody uses it in chat.
fn custom_number(n: Num, fmt: &str, l: &Locale) -> String {
    let sections = split_sections(fmt);
    let d0 = Digits::of(n);
    let (sec, neg_sign) = match sections.len() {
        1 => (&sections[0], d0.neg),
        _ if d0.is_zero() && sections.len() > 2 => (&sections[2], false),
        _ if d0.neg => (&sections[1], false),
        _ => (&sections[0], false),
    };
    // Parse the section: literal pieces and placeholders.
    #[derive(PartialEq)]
    enum T {
        Lit(String),
        Digit(bool), // true: '0'
        Dot,
    }
    let mut toks = Vec::new();
    let (mut grouping, mut scale, mut percent, mut permille) = (false, 0, false, false);
    let chars: Vec<char> = sec.chars().collect();
    let mut i = 0;
    let mut seen_dot = false;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '0' | '#' => toks.push(T::Digit(c == '0')),
            '.' if !seen_dot => {
                seen_dot = true;
                toks.push(T::Dot);
            }
            ',' => {
                // Commas right before the point (or the end) divide by 1000; between digits, group.
                let next = chars[i + 1..].iter().find(|&&c| c != ',');
                if matches!(next, Some('0' | '#')) && toks.iter().any(|t| matches!(t, T::Digit(_))) && !seen_dot {
                    grouping = true;
                } else if !seen_dot {
                    scale += 1;
                }
            }
            '%' => {
                percent = true;
                toks.push(T::Lit("%".into()));
            }
            '‰' => {
                permille = true;
                toks.push(T::Lit("‰".into()));
            }
            '\\' if i + 1 < chars.len() => {
                i += 1;
                toks.push(T::Lit(chars[i].to_string()));
            }
            '\'' | '"' => {
                let end = chars[i + 1..]
                    .iter()
                    .position(|&x| x == c)
                    .map_or(chars.len(), |p| i + 1 + p);
                toks.push(T::Lit(chars[i + 1..end].iter().collect()));
                i = end;
            }
            _ => toks.push(T::Lit(c.to_string())),
        }
        i += 1;
    }
    let dot = toks.iter().position(|t| *t == T::Dot).unwrap_or(toks.len());
    let int_ph: Vec<bool> = toks[..dot]
        .iter()
        .filter_map(|t| if let T::Digit(z) = t { Some(*z) } else { None })
        .collect();
    let frac_ph: Vec<bool> = toks[dot..]
        .iter()
        .filter_map(|t| if let T::Digit(z) = t { Some(*z) } else { None })
        .collect();
    let mut d = Digits::of(n);
    d.neg = false;
    d.exp += if percent { 2 } else { 0 } + if permille { 3 } else { 0 } - 3 * scale;
    let fmax = frac_ph.len();
    let fmin = frac_ph.iter().rposition(|&z| z).map_or(0, |p| p + 1);
    d.round_frac(fmax as i32);
    let mut frac = d.frac_part(fmax);
    while frac.len() > fmin && frac.ends_with('0') {
        frac.pop();
    }
    let mut int = d.int_part();
    let need = int_ph.iter().position(|&z| z).map_or(0, |p| int_ph.len() - p);
    if int == "0" && need == 0 {
        int.clear();
    }
    if int.len() < need {
        int = format!("{int:0>need$}");
    }
    // Lay it out: integer digits right to left into the integer placeholders (the first one takes
    // all that are left, as "(###) ###-####" does), fraction digits one per placeholder.
    let places = int_ph.len();
    let digits: Vec<char> = int.chars().collect();
    let lead = (digits.len() + 1).saturating_sub(places);
    let mut out = String::new();
    let push_digit = |out: &mut String, i: usize| {
        out.push(digits[i]);
        let right = digits.len() - 1 - i;
        if grouping && right > 0 && right.is_multiple_of(3) {
            *out += &l.group;
        }
    };
    let (mut slot, mut fchars) = (0, frac.chars());
    for (k, t) in toks.iter().enumerate() {
        match t {
            T::Lit(s) => out += s,
            T::Digit(_) if k < dot => {
                if slot == 0 {
                    (0..lead).for_each(|i| push_digit(&mut out, i));
                } else if let Some(i) = (slot + digits.len()).checked_sub(places) {
                    push_digit(&mut out, i);
                }
                slot += 1;
            }
            T::Digit(_) => out.extend(fchars.next()),
            T::Dot => {
                if places == 0 {
                    (0..digits.len()).for_each(|i| push_digit(&mut out, i));
                }
                if !frac.is_empty() {
                    out += &l.dec;
                }
            }
        }
    }
    let all_zero = int.chars().all(|c| !c.is_ascii_digit() || c == '0') && frac.chars().all(|c| c == '0');
    if neg_sign && !all_zero {
        format!("-{out}")
    } else {
        out
    }
}

/// `;` sections, outside quotes and escapes.
fn split_sections(fmt: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut q = None;
    let mut esc = false;
    for c in fmt.chars() {
        let cur = out.last_mut().unwrap();
        match c {
            _ if esc => {
                esc = false;
                cur.push(c);
            }
            '\\' => {
                esc = true;
                cur.push(c);
            }
            '\'' | '"' if q.is_none() => {
                q = Some(c);
                cur.push(c);
            }
            c2 if Some(c2) == q => {
                q = None;
                cur.push(c);
            }
            ';' if q.is_none() => out.push(String::new()),
            _ => cur.push(c),
        }
    }
    out
}

/// A date/time with a .NET format string (standard like "d" or "T", or custom like "dddd, dd MMMM").
pub fn date(t: &Time, fmt: &str) -> String {
    let l = locale();
    let pattern = match fmt {
        "d" => l.short_date.clone(),
        "D" => l.long_date.clone(),
        "f" => format!("{} {}", l.long_date, l.short_time),
        "F" | "U" => format!("{} {}", l.long_date, l.long_time),
        "g" => format!("{} {}", l.short_date, l.short_time),
        "G" => format!("{} {}", l.short_date, l.long_time),
        "M" | "m" => l.month_day.clone(),
        "O" | "o" => "yyyy'-'MM'-'dd'T'HH':'mm':'ss'.'fffffffzzz".into(),
        "R" | "r" => "ddd, dd MMM yyyy HH':'mm':'ss 'GMT'".into(),
        "s" => "yyyy'-'MM'-'dd'T'HH':'mm':'ss".into(),
        "t" => l.short_time.clone(),
        "T" => l.long_time.clone(),
        "u" => "yyyy'-'MM'-'dd HH':'mm':'ss'Z'".into(),
        "Y" | "y" => "MMMM yyyy".into(),
        _ => fmt.into(),
    };
    // R and r always use English names.
    let en = matches!(fmt, "R" | "r");
    custom_date(t, &pattern, l, en)
}

fn custom_date(t: &Time, f: &str, l: &Locale, english: bool) -> String {
    const EN_DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
    const EN_MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let chars: Vec<char> = f.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    // After a "%", the next letter is a field on its own ("%d", or "%h" in "%hmm").
    let mut single = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '%' && chars.get(i + 1).is_some_and(|&x| x != '%') {
            single = true;
            i += 1;
            continue;
        }
        let mut n = 1;
        while !single && i + n < chars.len() && chars[i + n] == c {
            n += 1;
        }
        single = false;
        let h12 = if t.hour.is_multiple_of(12) { 12 } else { t.hour % 12 };
        match c {
            'd' => {
                out += &match n {
                    1 => t.day.to_string(),
                    2 => format!("{:02}", t.day),
                    3 if english => EN_DAYS[t.weekday as usize % 7].to_string(),
                    3 => l.days_abbr[t.weekday as usize % 7].clone(),
                    _ => l.days[t.weekday as usize % 7].clone(),
                }
            }
            'M' => {
                let m = (t.month as usize + 11) % 12;
                out += &match n {
                    1 => t.month.to_string(),
                    2 => format!("{:02}", t.month),
                    3 if english => EN_MONTHS[m].to_string(),
                    3 => l.months_abbr[m].clone(),
                    _ => l.months[m].clone(),
                }
            }
            'y' => {
                out += &match n {
                    1 => (t.year % 100).to_string(),
                    2 => format!("{:02}", t.year % 100),
                    _ => format!("{:0w$}", t.year, w = n),
                }
            }
            'h' => out += &if n == 1 { h12.to_string() } else { format!("{h12:02}") },
            'H' => {
                out += &if n == 1 {
                    t.hour.to_string()
                } else {
                    format!("{:02}", t.hour)
                }
            }
            'm' => {
                out += &if n == 1 {
                    t.min.to_string()
                } else {
                    format!("{:02}", t.min)
                }
            }
            's' => {
                out += &if n == 1 {
                    t.sec.to_string()
                } else {
                    format!("{:02}", t.sec)
                }
            }
            'f' | 'F' => {
                let n = n.min(7);
                let s = format!("{:09}", t.nanos);
                let s = &s[..n];
                if c == 'F' {
                    let s = s.trim_end_matches('0');
                    // No fraction: no point before it either, as .NET writes it.
                    if s.is_empty() && out.ends_with('.') {
                        out.pop();
                    }
                    out += s;
                } else {
                    out += s;
                }
            }
            't' => {
                let s = if t.hour < 12 { &l.am } else { &l.pm };
                out += &if n == 1 {
                    s.chars().next().map(String::from).unwrap_or_default()
                } else {
                    s.clone()
                };
            }
            'z' => {
                let o = t.offset;
                let sign = if o < 0 { '-' } else { '+' };
                let (hh, mm) = (o.abs() / 3600, o.abs() / 60 % 60);
                out += &match n {
                    1 => format!("{sign}{hh}"),
                    2 => format!("{sign}{hh:02}"),
                    _ => format!("{sign}{hh:02}:{mm:02}"),
                }
            }
            'K' => {
                let o = t.offset;
                out += &format!(
                    "{}{:02}:{:02}",
                    if o < 0 { '-' } else { '+' },
                    o.abs() / 3600,
                    o.abs() / 60 % 60
                );
            }
            'g' => out += "A.D.",
            ':' => out += &":".repeat(n),
            '/' => out += &l.date_sep.repeat(n),
            '\'' | '"' => {
                // Up to the closing quote; a backslash in it escapes the next character.
                i += 1;
                while let Some(&x) = chars.get(i) {
                    i += 1;
                    match x {
                        _ if x == c => break,
                        '\\' => {
                            out.extend(chars.get(i));
                            i += 1;
                        }
                        _ => out.push(x),
                    }
                }
                continue;
            }
            '\\' => {
                out.extend(chars.get(i + 1));
                i += 2;
                continue;
            }
            _ => out.extend(std::iter::repeat_n(c, n)),
        }
        i += n;
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn hex_writes_two_lowercase_digits_a_byte() {
        let all: Vec<u8> = (0..=255).collect();
        let old: String = all.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(super::hex(&all), old);
        assert_eq!(super::hex(&[]), "");
        assert_eq!(super::hex(&[0, 0xab, 0xff]), "00abff");
    }

    #[test]
    fn parses_locale_decimal_numbers() {
        assert_eq!(super::parse_number_with_decimal("2,5", ","), Some(2.5));
        assert_eq!(super::parse_number_with_decimal("2.5", "."), Some(2.5));
        assert_eq!(super::parse_number_with_decimal("1.2,5", ","), None);
    }
    use super::*;

    fn en() -> &'static Locale {
        static L: OnceLock<Locale> = OnceLock::new();
        L.get_or_init(Locale::en_us)
    }

    #[test]
    fn numbers() {
        // The tests run with whatever locale the machine has: only check locale-free formats, and
        // the en-US ones through custom_number/g with an explicit locale.
        let f = |v: f64, s: &str| number(Num::Float(v), s);
        let i = |v: i64, s: &str| number(Num::Int(v), s);
        assert_eq!(f(1234.5678, "F1").unwrap(), format!("1234{}6", locale().dec));
        assert_eq!(i(42, "D5").unwrap(), "00042");
        assert_eq!(i(42, "X").unwrap(), "2A");
        assert_eq!(i(42, "x4").unwrap(), "002a");
        assert_eq!(f(1.5, "D"), None);
        assert_eq!(f(2.675, "F2").unwrap(), format!("2{}68", locale().dec)); // .NET Framework rounding
        assert_eq!(f(-0.001, "F2").unwrap(), format!("0{}00", locale().dec));
        assert_eq!(custom_number(Num::Float(1234.5678), "#,##0", en()), "1,235");
        assert_eq!(custom_number(Num::Float(1234.5678), "0.00", en()), "1234.57");
        assert_eq!(custom_number(Num::Float(0.5), "#.##", en()), ".5");
        assert_eq!(custom_number(Num::Int(7), "000", en()), "007");
        assert_eq!(custom_number(Num::Float(-3.0), "0;(0)", en()), "(3)");
        assert_eq!(custom_number(Num::Float(0.256), "0.0%", en()), "25.6%");
        assert_eq!(custom_number(Num::Int(1500000), "0,,.0M", en()), "1.5M");
        assert_eq!(custom_number(Num::Float(5.0), "'#'0", en()), "#5");
        assert_eq!(
            g(&Digits::of(Num::Float(1.0 / 3.0)), 15, 'E', en()),
            "0.333333333333333"
        );
        assert_eq!(g(&Digits::of(Num::Float(1e20)), 15, 'E', en()), "1E+20");
        assert_eq!(g(&Digits::of(Num::Float(0.00001)), 15, 'E', en()), "1E-05");
        assert_eq!(g(&Digits::of(Num::Float(0.1 + 0.2)), 15, 'E', en()), "0.3");
        assert_eq!(
            g(&Digits::of(Num::Float(2.0 * std::f64::consts::PI)), 15, 'E', en()),
            "6.28318530717959"
        );
        assert_eq!(g(&Digits::of(Num::Float(-3.0)), 15, 'E', en()), "-3");
    }

    #[test]
    fn dates() {
        let t = Time {
            year: 2026,
            month: 10,
            day: 11,
            hour: 16,
            min: 42,
            sec: 5,
            nanos: 750_000_000,
            weekday: 0,
            offset: -3 * 3600,
        };
        let d = |f: &str| custom_date(&t, f, en(), false);
        assert_eq!(d("yyyy/MM/dd"), "2026/10/11");
        assert_eq!(d("hh:mm"), "04:42");
        assert_eq!(d("dddd, dd MMMM yyyy"), "Sunday, 11 October 2026");
        assert_eq!(d("yyyyMMdd.hhmmss"), "20261011.044205");
        assert_eq!(d("yyyy-MM-ddTHH:mm:ss.fffzzz"), "2026-10-11T16:42:05.750-03:00");
        assert_eq!(d("MMM ddd tt"), "Oct Sun PM");
        assert_eq!(d("h:mm tt 'at' d/M"), "4:42 PM at 11/10");
        assert_eq!(d("%d"), "11");
        assert_eq!(custom_date(&t, "ddd, dd MMM", en(), true), "Sun, 11 Oct");
        assert_eq!(strftime_to_net("%m/%d/%Y"), "MM/dd/yyyy");
        assert_eq!(strftime_to_net("%r"), "hh:mm:ss tt");
    }

    fn at(year: i32, month: u32, day: u32, hour: u32, min: u32) -> Time {
        let days = days_from_civil(i64::from(year), month, day);
        Time {
            hour,
            min,
            ..crate::engine::civil_time(days * 86_400)
        }
    }

    #[test]
    fn zero_as_a_percent_is_one_zero() {
        assert_eq!(number(Num::Int(0), "P").unwrap(), "0.00%");
        assert_eq!(number(Num::Float(0.0), "P0").unwrap(), "0%");
        assert_eq!(custom_number(Num::Int(0), "0.0%", en()), "0.0%");
        assert_eq!(custom_number(Num::Float(0.0), "0‰", en()), "0‰");
        assert_eq!(number(Num::Float(0.5), "P1").unwrap(), "50.0%");
    }

    #[test]
    fn infinity_and_nan_are_written_not_zero() {
        let f = |v: f64, s: &str| number(Num::Float(v), s);
        assert_eq!(f(f64::NAN, "N2").as_deref(), Some("NaN"));
        assert_eq!(f(f64::INFINITY, "C").as_deref(), Some("Infinity"));
        assert_eq!(f(f64::NEG_INFINITY, "0.00").as_deref(), Some("-Infinity"));
        assert_eq!(f(f64::NAN, "R").as_deref(), Some("NaN"));
        assert_eq!(f(f64::NAN, "D"), None);
        assert_eq!(f(f64::INFINITY, "X2"), None);
    }

    #[test]
    fn integer_placeholders_take_one_digit_each() {
        let c = |v: i64, s: &str| custom_number(Num::Int(v), s, en());
        assert_eq!(c(5_551_234_567, "(###) ###-####"), "(555) 123-4567");
        assert_eq!(c(12, "##-##"), "-12");
        assert_eq!(c(12, "00-00"), "00-12");
        assert_eq!(c(123_456, "##-##"), "1234-56");
        assert_eq!(c(1_234_567, "#,##0"), "1,234,567");
        assert_eq!(c(-1_234_567, "#,##0"), "-1,234,567");
        assert_eq!(c(0, "#,##0"), "0");
        assert_eq!(c(0, "#.##"), "");
        assert_eq!(c(7, "abc"), "abc");
        assert_eq!(custom_number(Num::Float(1234.5), "0,0.00", en()), "1,234.50");
        assert_eq!(custom_number(Num::Float(12.5), ".00", en()), "12.50");
        assert_eq!(custom_number(Num::Float(1234.5), "#,#.00", en()), "1,234.50");
    }

    #[test]
    fn integers_round_trip_through_every_integer_format() {
        let mut samples: Vec<i64> = (-1200..1200).collect();
        samples.extend([i64::MIN, i64::MAX, 999_999, 1_000_000, -1_000_000, 123_456_789_012]);
        for v in samples {
            let n = Num::Int(v);
            assert_eq!(number(n, "D").unwrap(), v.to_string(), "D {v}");
            assert_eq!(number(n, "N0").unwrap().replace(',', ""), v.to_string(), "N0 {v}");
            assert_eq!(number(n, "R").unwrap(), v.to_string(), "R {v}");
            let grouped = custom_number(n, "#,##0", en());
            assert_eq!(grouped.replace(',', ""), v.to_string(), "#,##0 {v}");
            // Groups of three, the first of one to three digits.
            let digits = grouped.trim_start_matches('-');
            let parts: Vec<&str> = digits.split(',').collect();
            assert!((1..=3).contains(&parts[0].len()), "{grouped}");
            assert!(parts[1..].iter().all(|p| p.len() == 3), "{grouped}");
        }
    }

    #[test]
    fn exact_fractions_match_rusts_formatting() {
        // Quarters are exact in binary, so no rounding question: the two must agree.
        for i in -2000..2000 {
            let v = f64::from(i) / 4.0;
            assert_eq!(custom_number(Num::Float(v), "0.00", en()), format!("{v:.2}"), "{v}");
            let fixed = number(Num::Float(v), "F2").unwrap();
            assert_eq!(fixed, format!("{v:.2}"), "{v}");
        }
    }

    #[test]
    fn halves_round_away_from_zero() {
        for (v, want) in [
            (0.5, "1"),
            (1.5, "2"),
            (2.5, "3"),
            (-0.5, "-1"),
            (-2.5, "-3"),
            (0.49, "0"),
        ] {
            assert_eq!(number(Num::Float(v), "F0").unwrap(), want, "{v}");
        }
        assert_eq!(number(Num::Float(9.995), "N2").unwrap(), "10.00");
        assert_eq!(number(Num::Float(999_999.5), "N0").unwrap(), "1,000,000");
    }

    #[test]
    fn a_percent_sign_makes_the_next_letter_a_field_alone() {
        let t = at(2026, 3, 9, 16, 5);
        let d = |f: &str| custom_date(&t, f, en(), false);
        assert_eq!(d("%h"), "4");
        assert_eq!(d("%Hmm"), "1605");
        assert_eq!(d("%d/%M"), "9/3");
        assert_eq!(d("%%"), "%%");
        assert_eq!(d("50%"), "50%");
    }

    #[test]
    fn an_empty_big_f_fraction_takes_its_point_away() {
        let mut t = at(2026, 3, 9, 16, 5);
        let d = |t: &Time, f: &str| custom_date(t, f, en(), false);
        assert_eq!(d(&t, "HH:mm:ss.FFF"), "16:05:00");
        t.nanos = 750_000_000;
        assert_eq!(d(&t, "HH:mm:ss.FFF"), "16:05:00.75");
        assert_eq!(d(&t, "ss.fff"), "00.750");
    }

    #[test]
    fn quotes_and_escapes_are_literal() {
        let t = at(2026, 3, 9, 16, 5);
        let d = |f: &str| custom_date(&t, f, en(), false);
        assert_eq!(d(r"'It\'s' h"), "It's 4");
        assert_eq!(d(r"\h h"), "h 4");
        assert_eq!(d("\"dd\" dd"), "dd 09");
        assert_eq!(d("'unclosed"), "unclosed");
        assert_eq!(d("dd\\"), "09");
    }

    #[test]
    fn a_picked_slash_is_a_slash_and_quotes_are_kept() {
        assert_eq!(literal_slashes("dd/MM/yyyy"), r"dd\/MM\/yyyy");
        assert_eq!(literal_slashes("'a/b' d/M"), r"'a/b' d\/M");
        assert_eq!(literal_slashes(r"d\/M"), r"d\/M");
        let dotted = Locale {
            date_sep: ".".into(),
            ..Locale::en_us()
        };
        let t = at(2026, 3, 9, 16, 5);
        assert_eq!(
            custom_date(&t, &literal_slashes("dd/MM 'x/y'"), &dotted, false),
            "09/03 x/y"
        );
        assert_eq!(custom_date(&t, "dd/MM", &dotted, false), "09.03");
    }

    #[test]
    fn glibc_patterns_become_dotnet_ones() {
        assert_eq!(strftime_to_net("%Hh%M"), r"HH\hmm");
        assert_eq!(strftime_to_net("%l:%M %P"), "h:mm tt");
        assert_eq!(strftime_to_net("%OH:%OM"), "HH:mm");
        assert_eq!(strftime_to_net("%Ey"), "yy");
        assert_eq!(strftime_to_net("%F %%"), r"yyyy-MM-dd \%");
        assert_eq!(strftime_to_net("%d.%m.%Y"), "dd.MM.yyyy");
        assert_eq!(strftime_to_net("%Y年%m月%d日"), "yyyy年MM月dd日");
        assert_eq!(strftime_to_net("%k:%M"), "H:mm");
        let t = at(2026, 3, 9, 16, 5);
        assert_eq!(custom_date(&t, &strftime_to_net("%Hh%M"), en(), false), "16h05");
        assert_eq!(custom_date(&t, &strftime_to_net("%F %%"), en(), false), "2026-03-09 %");
        assert_eq!(date_separator("dd.MM.yyyy"), ".");
        assert_eq!(date_separator(r"yyyy\-MM"), "-");
        assert_eq!(date_separator("yyyy年MM月dd日"), "/");
    }

    #[test]
    fn os_values_left_empty_keep_the_default() {
        let mut f = "x".to_string();
        keep(&mut f, String::new());
        assert_eq!(f, "x");
        keep(&mut f, ",".into());
        assert_eq!(f, ",");
    }

    #[test]
    fn twelve_hour_clock_and_offsets() {
        let d = |t: &Time, f: &str| custom_date(t, f, en(), false);
        let mut t = at(2024, 2, 29, 0, 7);
        assert_eq!(d(&t, "h:mm tt dddd"), "12:07 AM Thursday");
        t.hour = 12;
        assert_eq!(d(&t, "hh t"), "12 P");
        t.hour = 23;
        assert_eq!(d(&t, "h tt"), "11 PM");
        t.offset = -(3 * 3600 + 1800);
        assert_eq!(
            (d(&t, "z"), d(&t, "zz"), d(&t, "zzz"), d(&t, "K")),
            ("-3".into(), "-03".into(), "-03:30".into(), "-03:30".into())
        );
        t.offset = 5 * 3600;
        assert_eq!(d(&t, "zz zzz"), "+05 +05:00");
        assert_eq!(d(&at(2000, 1, 1, 0, 0), "yy y yyy yyyyy"), "00 0 2000 02000");
    }

    #[test]
    fn dates_before_1970_and_leap_days() {
        let t = crate::engine::civil_time(-1);
        assert_eq!(
            custom_date(&t, "yyyy-MM-dd HH:mm:ss dddd", en(), false),
            "1969-12-31 23:59:59 Wednesday"
        );
        assert_eq!(custom_date(&at(1900, 3, 1, 0, 0), "dddd", en(), false), "Thursday");
        assert_eq!(custom_date(&at(2100, 2, 28, 0, 0), "ddd", en(), false), "Sun");
        // Every day, from 1600 to 2400, comes back to its day number.
        for days in (-135_000..160_000).step_by(37) {
            let t = crate::engine::civil_time(days * 86_400);
            assert_eq!(days_from_civil(i64::from(t.year), t.month, t.day), days, "{t:?}");
        }
    }

    #[test]
    fn a_local_time_turns_to_utc_with_the_offset_of_its_day() {
        // UTC+0 until `jump`, UTC+1 after (a daylight saving start).
        let jump = 1_000_000_000;
        let dst = |s: i64| if s >= jump { 3600 } else { 0 };
        assert_eq!(from_local_with(jump - 36_000, dst), jump - 36_000);
        assert_eq!(from_local_with(jump + 36_000, dst), jump + 32_400);
        for secs in [0, 1_700_000_000, 1_750_000_000] {
            let t = local_time(secs);
            let naive = secs + i64::from(t.offset);
            assert_eq!(from_local(naive), secs);
        }
    }

    #[test]
    fn a_local_time_carries_its_own_offset() {
        for secs in [-86_400 * 400, 0, 1_700_000_000, 1_720_000_000, 1_735_000_000] {
            let t = local_time(secs);
            assert_eq!(t.offset, offset_at(secs));
            let back = days_from_civil(i64::from(t.year), t.month, t.day) * 86_400
                + i64::from(t.hour * 3600 + t.min * 60 + t.sec)
                - i64::from(t.offset);
            assert_eq!(back, secs);
        }
        assert_eq!(offset_at(unix_now().as_secs() as i64), now().offset);
    }
}

/// A Windows error as a person reads it: its message, or (when Windows has none, as for a missing
/// component) what it means, with its code.
#[cfg(windows)]
pub fn win_error(e: &windows::core::Error) -> String {
    let code = e.code().0 as u32;
    let msg = e.message();
    let msg = msg.trim();
    match code {
        // REGDB_E_CLASSNOTREG, CLASS_E_CLASSNOTAVAILABLE: the component isn't installed.
        0x8004_0154 | 0x8004_0111 => format!("not available on this system ({code:#010x})"),
        _ if msg.is_empty() => format!("error {code:#010x}"),
        _ => format!("{msg} ({code:#010x})"),
    }
}

#[cfg(all(test, windows))]
mod win_tests {
    /// A missing component says so, not a bare code or nothing (SAPI and the media session under Wine).
    #[test]
    fn windows_errors_read_as_words() {
        use windows::core::{Error, HRESULT};
        let missing = Error::from_hresult(HRESULT(0x8004_0154_u32 as i32));
        assert_eq!(super::win_error(&missing), "not available on this system (0x80040154)");
        let other = super::win_error(&Error::from_hresult(HRESULT(0x8007_0005_u32 as i32)));
        assert!(other.ends_with("(0x80070005)") && other.len() > 13, "{other}");
    }
}
