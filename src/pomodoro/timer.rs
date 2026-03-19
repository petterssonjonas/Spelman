use std::time::{Duration, Instant};

/// Which timer display style to use during breaks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimerStyle {
    Analog,
    Hourglass,
    Digital,
}

/// Pomodoro session phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PomodoroPhase {
    /// Work session — music plays.
    Work,
    /// Short break (5 min default) — music pauses.
    ShortBreak,
    /// Long break (15 min default) — after every 4 work sessions.
    LongBreak,
}

/// Actions the pomodoro wants the app to take.
#[derive(Debug, Clone, PartialEq)]
pub enum PomodoroAction {
    None,
    /// Pause the user's music — break is starting.
    PauseMusic,
    /// Resume the user's music — work session starting.
    ResumeMusic,
}

/// Pomodoro timer state machine.
#[derive(Debug, Clone)]
pub struct PomodoroTimer {
    pub active: bool,
    pub phase: PomodoroPhase,
    phase_start: Option<Instant>,
    accumulated: Duration,
    pub ticking: bool,
    pub sessions_completed: u32,
    pub work_duration: Duration,
    pub short_break_duration: Duration,
    pub long_break_duration: Duration,
    pub sessions_before_long_break: u32,
    pub timer_style: TimerStyle,
    pub break_ended: bool,
}

impl Default for PomodoroTimer {
    fn default() -> Self {
        Self {
            active: false,
            phase: PomodoroPhase::Work,
            phase_start: None,
            accumulated: Duration::ZERO,
            ticking: false,
            sessions_completed: 0,
            work_duration: Duration::from_secs(25 * 60),
            short_break_duration: Duration::from_secs(5 * 60),
            long_break_duration: Duration::from_secs(15 * 60),
            sessions_before_long_break: 4,
            timer_style: TimerStyle::Analog,
            break_ended: false,
        }
    }
}

impl PomodoroTimer {
    pub fn start(&mut self) {
        self.active = true;
        self.ticking = true;
        self.phase = PomodoroPhase::Work;
        self.phase_start = Some(Instant::now());
        self.accumulated = Duration::ZERO;
        self.sessions_completed = 0;
        self.break_ended = false;
    }

    pub fn stop(&mut self) {
        self.active = false;
        self.ticking = false;
        self.phase_start = None;
        self.accumulated = Duration::ZERO;
        self.break_ended = false;
    }

    pub fn toggle_pause(&mut self) {
        if !self.active {
            return;
        }
        if self.ticking {
            if let Some(start) = self.phase_start {
                self.accumulated += start.elapsed();
            }
            self.phase_start = None;
            self.ticking = false;
        } else {
            self.phase_start = Some(Instant::now());
            self.ticking = true;
        }
    }

    pub fn skip(&mut self) -> PomodoroAction {
        self.advance_phase()
    }

    pub fn cycle_style(&mut self) {
        self.timer_style = match self.timer_style {
            TimerStyle::Analog => TimerStyle::Hourglass,
            TimerStyle::Hourglass => TimerStyle::Digital,
            TimerStyle::Digital => TimerStyle::Analog,
        };
    }

    pub fn elapsed(&self) -> Duration {
        let running = self
            .phase_start
            .map(|s| s.elapsed())
            .unwrap_or(Duration::ZERO);
        self.accumulated + running
    }

    pub fn phase_duration(&self) -> Duration {
        match self.phase {
            PomodoroPhase::Work => self.work_duration,
            PomodoroPhase::ShortBreak => self.short_break_duration,
            PomodoroPhase::LongBreak => self.long_break_duration,
        }
    }

    pub fn remaining(&self) -> Duration {
        self.phase_duration().saturating_sub(self.elapsed())
    }

    pub fn fraction(&self) -> f64 {
        let total = self.phase_duration().as_secs_f64();
        if total <= 0.0 {
            return 0.0;
        }
        (self.elapsed().as_secs_f64() / total).clamp(0.0, 1.0)
    }

    /// Tick the timer. Call every frame. Returns action if phase transition occurred.
    pub fn tick(&mut self) -> PomodoroAction {
        if !self.active || !self.ticking {
            return PomodoroAction::None;
        }
        if self.elapsed() >= self.phase_duration() {
            return self.advance_phase();
        }
        PomodoroAction::None
    }

    fn advance_phase(&mut self) -> PomodoroAction {
        self.accumulated = Duration::ZERO;
        self.phase_start = Some(Instant::now());
        self.break_ended = false;

        match self.phase {
            PomodoroPhase::Work => {
                self.sessions_completed += 1;
                let cycle = self.sessions_before_long_break.max(1);
                if self.sessions_completed % cycle == 0 {
                    self.phase = PomodoroPhase::LongBreak;
                } else {
                    self.phase = PomodoroPhase::ShortBreak;
                }
                PomodoroAction::PauseMusic
            }
            PomodoroPhase::ShortBreak | PomodoroPhase::LongBreak => {
                self.phase = PomodoroPhase::Work;
                PomodoroAction::ResumeMusic
            }
        }
    }

    pub fn phase_label(&self) -> &'static str {
        match self.phase {
            PomodoroPhase::Work => "Work",
            PomodoroPhase::ShortBreak => "Short Break",
            PomodoroPhase::LongBreak => "Long Break",
        }
    }

    pub fn remaining_display(&self) -> String {
        let remaining = self.remaining();
        let mins = remaining.as_secs() / 60;
        let secs = remaining.as_secs() % 60;
        format!("{mins:02}:{secs:02}")
    }
}
