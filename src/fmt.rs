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

pub fn locale() -> &'static Locale {
    static L: OnceLock<Locale> = OnceLock::new();
    L.get_or_init(os_locale)
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
    out.dec = get(0x10000);
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
    out.date_sep = out
        .short_date
        .chars()
        .find(|c| !c.is_alphanumeric())
        .map_or("/".into(), String::from);
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
    out.dec = get(0x0E);
    out.group = get(0x0F);
    out.currency = get(0x14);
    let pos = get(0x1B); // 0 "$1", 1 "1$", 2 "$ 1", 3 "1 $"
    out.currency_after = pos == "1" || pos == "3";
    out.currency_space = pos == "2" || pos == "3";
    out.date_sep = get(0x1D);
    for i in 0..12 {
        out.months[i] = get(0x38 + i as u32);
        out.months_abbr[i] = get(0x44 + i as u32);
    }
    // Windows starts its day names on Monday.
    for i in 0..7 {
        out.days[(i + 1) % 7] = get(0x2A + i as u32);
        out.days_abbr[(i + 1) % 7] = get(0x31 + i as u32);
    }
    out.am = get(0x28);
    out.pm = get(0x29);
    out.short_date = get(0x1F);
    out.long_date = get(0x20);
    out.long_time = get(0x1003);
    out.short_time = get(0x79);
    out.set_order();
    out.long_date = get(0x20);
    out
}

#[cfg(not(any(target_os = "linux", windows)))]
fn os_locale() -> Locale {
    Locale::en_us()
}

/// A strftime pattern (glibc's D_FMT/T_FMT) as a .NET one.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn strftime_to_net(f: &str) -> String {
    let mut out = String::new();
    let mut it = f.chars();
    while let Some(c) = it.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        out += match it.next() {
            Some('d') => "dd",
            Some('e') => "d",
            Some('m') => "MM",
            Some('Y') => "yyyy",
            Some('y') => "yy",
            Some('H') => "HH",
            Some('I') => "hh",
            Some('M') => "mm",
            Some('S') => "ss",
            Some('p') => "tt",
            Some('r') => "hh:mm:ss tt",
            Some('T') => "HH:mm:ss",
            Some('R') => "HH:mm",
            Some('D') => "MM/dd/yy",
            Some('b') | Some('h') => "MMM",
            Some('B') => "MMMM",
            Some('a') => "ddd",
            Some('A') => "dddd",
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

pub fn unix_now() -> std::time::Duration {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
}

/// Now, in the local time zone.
#[cfg(unix)]
pub fn now() -> Time {
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
    extern "C" {
        fn localtime_r(t: *const i64, tm: *mut Tm) -> *mut Tm;
    }
    let d = unix_now();
    let secs = d.as_secs() as i64;
    // SAFETY: Tm matches glibc's struct tm on 64-bit; zeroed is a valid value for it.
    let mut tm: Tm = unsafe { std::mem::zeroed() };
    unsafe { localtime_r(&secs, &mut tm) };
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
        if self.exp <= 0 {
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
    if grouping {
        int = group(&int, &l.group);
    }
    // Lay it out: the whole integer at the first integer placeholder, other integer placeholders
    // print nothing; fraction digits one per placeholder.
    let mut out = String::new();
    let (mut int_done, mut fi) = (false, 0);
    let mut fchars = frac.chars();
    for (k, t) in toks.iter().enumerate() {
        match t {
            T::Lit(s) => out += s,
            T::Digit(_) if k < dot => {
                if !int_done {
                    out += &int;
                    int_done = true;
                }
            }
            T::Digit(_) => {
                if let Some(c) = fchars.next() {
                    out.push(c);
                }
                fi += 1;
            }
            T::Dot => {
                if !int_done {
                    out += &int;
                    int_done = true;
                }
                if !frac.is_empty() {
                    out += &l.dec;
                }
            }
        }
    }
    let _ = fi;
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
    // A lone "%x" is the custom specifier x.
    let chars = if chars.len() == 2 && chars[0] == '%' {
        chars[1..].to_vec()
    } else {
        chars
    };
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let mut n = 1;
        while i + n < chars.len() && chars[i + n] == c {
            n += 1;
        }
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
                out += &if c == 'F' {
                    s.trim_end_matches('0').to_string()
                } else {
                    s.to_string()
                };
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
                let end = chars[i + 1..]
                    .iter()
                    .position(|&x| x == c)
                    .map_or(chars.len(), |p| i + 1 + p);
                out.extend(&chars[i + 1..end]);
                i = end + 1;
                continue;
            }
            '\\' => {
                if let Some(&x) = chars.get(i + 1) {
                    out.push(x);
                }
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
}
