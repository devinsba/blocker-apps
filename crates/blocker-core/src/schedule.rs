//! When a rule is active.
//!
//! A [`Schedule`] is either [`Schedule::Always`] or a list of weekly
//! [`TimeWindow`]s evaluated in local time.
//!
//! ```toml
//! schedule = { type = "windows", windows = [{ days = ["mon","tue","wed","thu","fri"], start = "09:00", end = "17:00" }] }
//! ```

use std::fmt;
use std::str::FromStr;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{Datelike, Timelike};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const MINUTES_PER_DAY: u16 = 24 * 60;

/// When a rule applies.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Schedule {
    /// Always active.
    #[default]
    Always,
    /// Active inside any of the windows. No windows means never active.
    Windows { windows: Vec<TimeWindow> },
}

/// A weekly recurring window of local time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Days the window STARTS on. Empty = every day.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub days: Vec<Day>,
    /// Inclusive start.
    pub start: HhMm,
    /// Exclusive end. Earlier than `start` means the window runs overnight
    /// into the next day; equal to `start` means a full 24 hours.
    pub end: HhMm,
}

/// Day of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Day {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Day {
    const ALL: [Day; 7] = [
        Day::Mon,
        Day::Tue,
        Day::Wed,
        Day::Thu,
        Day::Fri,
        Day::Sat,
        Day::Sun,
    ];

    /// The following day (Sun wraps to Mon).
    pub fn next(self) -> Day {
        Day::ALL[(self.index() + 1) % 7]
    }

    fn prev(self) -> Day {
        Day::ALL[(self.index() + 6) % 7]
    }

    fn index(self) -> usize {
        self as usize
    }

    fn short(self) -> &'static str {
        match self {
            Day::Mon => "mon",
            Day::Tue => "tue",
            Day::Wed => "wed",
            Day::Thu => "thu",
            Day::Fri => "fri",
            Day::Sat => "sat",
            Day::Sun => "sun",
        }
    }
}

impl From<chrono::Weekday> for Day {
    fn from(w: chrono::Weekday) -> Self {
        match w {
            chrono::Weekday::Mon => Day::Mon,
            chrono::Weekday::Tue => Day::Tue,
            chrono::Weekday::Wed => Day::Wed,
            chrono::Weekday::Thu => Day::Thu,
            chrono::Weekday::Fri => Day::Fri,
            chrono::Weekday::Sat => Day::Sat,
            chrono::Weekday::Sun => Day::Sun,
        }
    }
}

impl FromStr for Day {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let l = s.trim().to_ascii_lowercase();
        Ok(match l.as_str() {
            "mon" | "monday" => Day::Mon,
            "tue" | "tues" | "tuesday" => Day::Tue,
            "wed" | "wednesday" => Day::Wed,
            "thu" | "thur" | "thurs" | "thursday" => Day::Thu,
            "fri" | "friday" => Day::Fri,
            "sat" | "saturday" => Day::Sat,
            "sun" | "sunday" => Day::Sun,
            _ => bail!("unknown day {s:?}"),
        })
    }
}

/// Minutes since midnight, (de)serialized as `"HH:MM"` (00:00 to 23:59).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct HhMm(pub u16);

impl FromStr for HhMm {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        let (h, m) = s
            .trim()
            .split_once(':')
            .ok_or_else(|| anyhow!("invalid time {s:?}, expected HH:MM"))?;
        let digits =
            |p: &str| !p.is_empty() && p.len() <= 2 && p.bytes().all(|b| b.is_ascii_digit());
        if !digits(h) || m.len() != 2 || !digits(m) {
            bail!("invalid time {s:?}, expected HH:MM");
        }
        let (h, m): (u16, u16) = (h.parse()?, m.parse()?);
        if h > 23 || m > 59 {
            bail!("time {s:?} out of range (00:00 to 23:59)");
        }
        Ok(HhMm(h * 60 + m))
    }
}

impl fmt::Display for HhMm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:02}:{:02}", self.0 / 60, self.0 % 60)
    }
}

impl Serialize for HhMm {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for HhMm {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl TimeWindow {
    fn lists(&self, day: Day) -> bool {
        self.days.is_empty() || self.days.contains(&day)
    }

    /// Is this window active at this weekday + minute-of-day (0..1440)?
    ///
    /// `start <= minute < end` on a listed day. Overnight windows
    /// (`end <= start`) also cover `00:00..end` on the day after a listed day.
    pub fn is_active_at(&self, day: Day, minute: u16) -> bool {
        let (start, end) = (self.start.0, self.end.0);
        if start < end {
            self.lists(day) && (start..end).contains(&minute)
        } else {
            (self.lists(day) && minute >= start) || (self.lists(day.prev()) && minute < end)
        }
    }

    fn describe(&self) -> String {
        format!("{} {}-{}", describe_days(&self.days), self.start, self.end)
    }
}

fn describe_days(days: &[Day]) -> String {
    let mut set = [false; 7];
    for d in days {
        set[d.index()] = true;
    }
    if days.is_empty() || set.iter().all(|&b| b) {
        return "daily".into();
    }
    let mut parts = Vec::new();
    let mut i = 0;
    while i < 7 {
        if !set[i] {
            i += 1;
            continue;
        }
        let mut j = i;
        while j + 1 < 7 && set[j + 1] {
            j += 1;
        }
        match j - i {
            0 => parts.push(Day::ALL[i].short().to_string()),
            1 => {
                parts.push(Day::ALL[i].short().to_string());
                parts.push(Day::ALL[j].short().to_string());
            }
            _ => parts.push(format!("{}-{}", Day::ALL[i].short(), Day::ALL[j].short())),
        }
        i = j + 1;
    }
    parts.join(",")
}

impl Schedule {
    /// Pure: is the schedule active at this weekday + minute-of-day?
    pub fn is_active_at(&self, day: Day, minute: u16) -> bool {
        match self {
            Schedule::Always => true,
            Schedule::Windows { windows } => windows.iter().any(|w| w.is_active_at(day, minute)),
        }
    }

    /// Is the schedule active right now, in local time?
    pub fn is_active_now(&self) -> bool {
        let now = chrono::Local::now();
        let minute = (now.hour() * 60 + now.minute()) as u16;
        self.is_active_at(now.weekday().into(), minute.min(MINUTES_PER_DAY - 1))
    }

    /// Human summary, e.g. `"mon-fri 09:00-17:00; sat 22:00-02:00"`.
    pub fn describe(&self) -> String {
        match self {
            Schedule::Always => "always".into(),
            Schedule::Windows { windows } if windows.is_empty() => "never".into(),
            Schedule::Windows { windows } => windows
                .iter()
                .map(TimeWindow::describe)
                .collect::<Vec<_>>()
                .join("; "),
        }
    }
}

fn parse_days(s: &str) -> Result<Vec<Day>> {
    if s.eq_ignore_ascii_case("daily") {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for item in s.split(',') {
        if let Some((a, b)) = item.split_once('-') {
            let (a, b): (Day, Day) = (a.parse()?, b.parse()?);
            let mut d = a;
            loop {
                if !out.contains(&d) {
                    out.push(d);
                }
                if d == b {
                    break;
                }
                d = d.next();
            }
        } else {
            let d: Day = item.parse()?;
            if !out.contains(&d) {
                out.push(d);
            }
        }
    }
    out.sort_by_key(|d| d.index());
    Ok(out)
}

/// Parse CLI window syntax: `"mon-fri 09:00-17:00"`, `"sat,sun 10:00-12:00"`,
/// `"mon,wed-fri 08:30-12:00"`, `"daily 22:00-06:00"`, `"09:00-17:00"`.
pub fn parse_window(s: &str) -> Result<TimeWindow> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    let (days, times) = match tokens.as_slice() {
        [t] => (Vec::new(), *t),
        [d, t] => (
            parse_days(d).with_context(|| format!("in window {s:?}"))?,
            *t,
        ),
        _ => bail!("invalid window {s:?}, expected \"[days] HH:MM-HH:MM\""),
    };
    let (start, end) = times
        .split_once('-')
        .ok_or_else(|| anyhow!("invalid window {s:?}, expected \"[days] HH:MM-HH:MM\""))?;
    Ok(TimeWindow {
        days,
        start: start.parse().with_context(|| format!("in window {s:?}"))?,
        end: end.parse().with_context(|| format!("in window {s:?}"))?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(s: &str) -> TimeWindow {
        parse_window(s).unwrap()
    }

    fn t(s: &str) -> u16 {
        s.parse::<HhMm>().unwrap().0
    }

    #[derive(Debug, Deserialize, Serialize, PartialEq)]
    struct Wrap {
        #[serde(default)]
        schedule: Schedule,
    }

    #[test]
    fn toml_round_trip() {
        let src = r#"schedule = { type = "windows", windows = [{ days = ["mon","tue","wed","thu","fri"], start = "09:00", end = "17:00" }] }"#;
        let parsed: Wrap = toml::from_str(src).unwrap();
        assert_eq!(
            parsed.schedule,
            Schedule::Windows {
                windows: vec![TimeWindow {
                    days: vec![Day::Mon, Day::Tue, Day::Wed, Day::Thu, Day::Fri],
                    start: HhMm(540),
                    end: HhMm(1020),
                }]
            }
        );
        let out = toml::to_string(&parsed).unwrap();
        assert_eq!(toml::from_str::<Wrap>(&out).unwrap(), parsed);
    }

    #[test]
    fn toml_always_and_default() {
        assert_eq!(Schedule::default(), Schedule::Always);
        let d: Wrap = toml::from_str("").unwrap();
        assert_eq!(d.schedule, Schedule::Always);
        let a: Wrap = toml::from_str(r#"schedule = { type = "always" }"#).unwrap();
        assert_eq!(a.schedule, Schedule::Always);
        let out = toml::to_string(&a).unwrap();
        assert_eq!(toml::from_str::<Wrap>(&out).unwrap(), a);
    }

    #[test]
    fn toml_days_optional_and_bad_time() {
        let p: Wrap = toml::from_str(
            r#"schedule = { type = "windows", windows = [{ start = "09:00", end = "10:00" }] }"#,
        )
        .unwrap();
        match p.schedule {
            Schedule::Windows { windows } => assert!(windows[0].days.is_empty()),
            _ => panic!(),
        }
        assert!(toml::from_str::<Wrap>(
            r#"schedule = { type = "windows", windows = [{ start = "25:00", end = "10:00" }] }"#
        )
        .is_err());
    }

    #[test]
    fn hhmm_display_and_parse() {
        assert_eq!(HhMm(9 * 60 + 5).to_string(), "09:05");
        assert_eq!("9:05".parse::<HhMm>().unwrap(), HhMm(545));
        assert_eq!("23:59".parse::<HhMm>().unwrap(), HhMm(1439));
        for bad in ["24:00", "12:60", "1200", "ab:cd", "12:5", ":30", ""] {
            assert!(bad.parse::<HhMm>().is_err(), "{bad}");
        }
    }

    #[test]
    fn parse_examples() {
        use Day::*;
        assert_eq!(
            w("mon-fri 09:00-17:00"),
            TimeWindow {
                days: vec![Mon, Tue, Wed, Thu, Fri],
                start: HhMm(540),
                end: HhMm(1020)
            }
        );
        assert_eq!(w("sat,sun 10:00-12:00").days, vec![Sat, Sun]);
        assert_eq!(w("mon,wed-fri 08:30-12:00").days, vec![Mon, Wed, Thu, Fri]);
        assert_eq!(w("mon,wed-fri 08:30-12:00").start, HhMm(510));
        let d = w("daily 22:00-06:00");
        assert!(d.days.is_empty());
        assert_eq!((d.start, d.end), (HhMm(1320), HhMm(360)));
        assert!(w("09:00-17:00").days.is_empty());
        assert_eq!(w("Monday-FRIDAY 09:00-17:00").days.len(), 5);
        assert_eq!(w("fri-mon 09:00-17:00").days, vec![Mon, Fri, Sat, Sun]);
    }

    #[test]
    fn parse_invalid() {
        for bad in [
            "25:00-26:00",
            "mon-fri",
            "xyz 09:00-10:00",
            "09:00",
            "",
            "mon 09:00-10:00 extra",
            "mon 09:00-24:00",
        ] {
            assert!(parse_window(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn boundaries() {
        let win = w("mon-fri 09:00-17:00");
        assert!(!win.is_active_at(Day::Mon, t("08:59")));
        assert!(win.is_active_at(Day::Mon, t("09:00")));
        assert!(win.is_active_at(Day::Mon, t("16:59")));
        assert!(!win.is_active_at(Day::Mon, t("17:00")));
        assert!(!win.is_active_at(Day::Sat, t("12:00")));
    }

    #[test]
    fn overnight() {
        let win = w("fri 22:00-02:00");
        assert!(win.is_active_at(Day::Fri, t("22:00")));
        assert!(win.is_active_at(Day::Fri, t("23:59")));
        assert!(win.is_active_at(Day::Sat, t("00:00")));
        assert!(win.is_active_at(Day::Sat, t("01:59")));
        assert!(!win.is_active_at(Day::Sat, t("02:00")));
        assert!(!win.is_active_at(Day::Fri, t("01:00")));
        assert!(!win.is_active_at(Day::Sat, t("23:00")));
    }

    #[test]
    fn overnight_sun_to_mon() {
        let win = w("sun 23:00-01:00");
        assert!(win.is_active_at(Day::Sun, t("23:30")));
        assert!(win.is_active_at(Day::Mon, t("00:30")));
        assert!(!win.is_active_at(Day::Mon, t("01:00")));
        assert!(!win.is_active_at(Day::Sun, t("00:30")));
    }

    #[test]
    fn full_day_when_equal() {
        let win = w("mon 06:00-06:00");
        assert!(win.is_active_at(Day::Mon, t("06:00")));
        assert!(win.is_active_at(Day::Mon, t("23:59")));
        assert!(win.is_active_at(Day::Tue, t("05:59")));
        assert!(!win.is_active_at(Day::Tue, t("06:00")));
        assert!(!win.is_active_at(Day::Mon, t("05:59")));
    }

    #[test]
    fn wraparound_range() {
        let win = w("fri-mon 09:00-17:00");
        for d in [Day::Fri, Day::Sat, Day::Sun, Day::Mon] {
            assert!(win.is_active_at(d, t("10:00")), "{d:?}");
        }
        for d in [Day::Tue, Day::Wed, Day::Thu] {
            assert!(!win.is_active_at(d, t("10:00")), "{d:?}");
        }
    }

    #[test]
    fn schedule_semantics() {
        assert!(Schedule::Always.is_active_at(Day::Wed, 0));
        let empty = Schedule::Windows { windows: vec![] };
        assert!(!empty.is_active_at(Day::Wed, 600));
        let s = Schedule::Windows {
            windows: vec![w("mon 09:00-10:00"), w("tue 09:00-10:00")],
        };
        assert!(s.is_active_at(Day::Tue, t("09:30")));
        assert!(!s.is_active_at(Day::Wed, t("09:30")));
        let _ = Schedule::Always.is_active_now();
        assert!(!empty.is_active_now());
    }

    #[test]
    fn weekday_conversion() {
        assert_eq!(Day::from(chrono::Weekday::Sun), Day::Sun);
        assert_eq!(Day::from(chrono::Weekday::Wed), Day::Wed);
        assert_eq!(Day::Sun.next(), Day::Mon);
        assert_eq!(Day::Mon.next(), Day::Tue);
    }

    #[test]
    fn describe_output() {
        assert_eq!(Schedule::Always.describe(), "always");
        assert_eq!(Schedule::Windows { windows: vec![] }.describe(), "never");
        let s = Schedule::Windows {
            windows: vec![w("mon-fri 09:00-17:00"), w("sat 22:00-02:00")],
        };
        assert_eq!(s.describe(), "mon-fri 09:00-17:00; sat 22:00-02:00");
        let s = Schedule::Windows {
            windows: vec![w("09:00-17:00"), w("sat,sun 10:00-12:00")],
        };
        assert_eq!(s.describe(), "daily 09:00-17:00; sat,sun 10:00-12:00");
        let s = Schedule::Windows {
            windows: vec![w("mon,wed-fri 08:30-12:00")],
        };
        assert_eq!(s.describe(), "mon,wed-fri 08:30-12:00");
    }
}
