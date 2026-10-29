use std::time::{Duration, Instant};

pub const NAMES: [&str; 3] = ["喝水", "起來走走", "記得眨眼"];

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub minutes: [u32; 3],
    pub enabled: [bool; 3],
    pub startup: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            minutes: [30, 60, 20],
            enabled: [true; 3],
            startup: false,
        }
    }
}

impl Settings {
    pub fn decode(text: &str) -> Result<Self, String> {
        let values: Vec<_> = text.split_whitespace().collect();
        if values.len() != 8 || values[0] != "v1" {
            return Err("設定檔格式不正確".into());
        }
        let mut s = Self::default();
        for i in 0..3 {
            s.minutes[i] = values[1 + i].parse().map_err(|_| "提醒間隔不正確")?;
            if !(1..=1440).contains(&s.minutes[i]) {
                return Err("提醒間隔必須介於 1 到 1440 分鐘".into());
            }
            s.enabled[i] = parse_bool(values[4 + i])?;
        }
        s.startup = parse_bool(values[7])?;
        Ok(s)
    }

    pub fn encode(&self) -> String {
        format!(
            "v1 {} {} {} {} {} {} {}\n",
            self.minutes[0],
            self.minutes[1],
            self.minutes[2],
            self.enabled[0] as u8,
            self.enabled[1] as u8,
            self.enabled[2] as u8,
            self.startup as u8
        )
    }
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err("設定開關不正確".into()),
    }
}

pub struct Scheduler {
    pub settings: Settings,
    pub pending: [bool; 3],
    due: [Instant; 3],
    pub paused: bool,
    pub away: bool,
}

impl Scheduler {
    pub fn new(settings: Settings, now: Instant) -> Self {
        let mut s = Self {
            settings,
            pending: [false; 3],
            due: [now; 3],
            paused: false,
            away: false,
        };
        s.reset(now);
        s
    }
    fn interval(&self, i: usize) -> Duration {
        Duration::from_secs(self.settings.minutes[i] as u64 * 60)
    }
    pub fn reset(&mut self, now: Instant) {
        for i in 0..3 {
            if !self.pending[i] {
                self.due[i] = now + self.interval(i);
            }
        }
    }
    pub fn tick(&mut self, now: Instant) {
        if self.paused || self.away {
            return;
        }
        for i in 0..3 {
            if self.settings.enabled[i] && now >= self.due[i] {
                self.pending[i] = true;
            }
        }
    }
    pub fn acknowledge(&mut self, i: usize, now: Instant) {
        self.pending[i] = false;
        self.due[i] = now + self.interval(i);
    }
    pub fn next_delay(&self, now: Instant) -> Option<Duration> {
        if self.paused || self.away {
            return None;
        }
        (0..3)
            .filter(|&i| self.settings.enabled[i] && !self.pending[i])
            .map(|i| self.due[i].saturating_duration_since(now))
            .min()
    }
    pub fn apply(&mut self, settings: Settings, now: Instant) {
        for i in 0..3 {
            if !settings.enabled[i] {
                self.pending[i] = false;
            }
            if settings.enabled[i] != self.settings.enabled[i]
                || settings.minutes[i] != self.settings.minutes[i]
            {
                self.due[i] = now + Duration::from_secs(settings.minutes[i] as u64 * 60);
            }
        }
        self.settings = settings;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_reminders_do_not_repeat_and_acknowledgement_is_independent() {
        let now = Instant::now();
        let mut s = Scheduler::new(
            Settings {
                minutes: [1, 2, 3],
                ..Settings::default()
            },
            now,
        );
        s.tick(now + Duration::from_secs(120));
        assert_eq!(s.pending, [true, true, false]);
        s.acknowledge(0, now + Duration::from_secs(130));
        s.tick(now + Duration::from_secs(189));
        assert_eq!(s.pending, [false, true, true]);
        s.tick(now + Duration::from_secs(190));
        assert_eq!(s.pending, [true, true, true]);
        assert_eq!(s.next_delay(now), None);
    }
    #[test]
    fn pause_and_away_preserve_pending_without_catchup() {
        for away in [false, true] {
            let now = Instant::now();
            let mut s = Scheduler::new(Settings::default(), now);
            s.pending[0] = true;
            s.paused = !away;
            s.away = away;
            let later = now + Duration::from_secs(10000);
            s.tick(later);
            assert_eq!(s.pending, [true, false, false]);
            assert_eq!(s.next_delay(later), None);
            s.paused = false;
            s.away = false;
            s.reset(later);
            s.tick(later);
            assert_eq!(s.pending, [true, false, false]);
        }
    }
    #[test]
    fn settings_validate_and_roundtrip() {
        let s = Settings::default();
        assert_eq!(Settings::decode(&s.encode()).unwrap(), s);
        assert!(Settings::decode("v1 0 60 20 1 1 1 0").is_err());
        assert!(Settings::decode("v1 30 60 20 2 1 1 0").is_err());
    }
    #[test]
    fn disabling_clears_pending_without_resetting_other_deadlines() {
        let now = Instant::now();
        let mut s = Scheduler::new(Settings::default(), now);
        s.pending[0] = true;
        let mut settings = s.settings.clone();
        settings.enabled[0] = false;
        s.apply(settings, now + Duration::from_secs(60));
        assert!(!s.pending[0]);
        assert_eq!(s.next_delay(now), Some(Duration::from_secs(1200)));
    }
}
