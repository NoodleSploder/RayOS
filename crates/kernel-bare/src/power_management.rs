/// Power Management & Dynamic Frequency Scaling
///
/// Intelligent power management with performance awareness
/// C-states and P-state management for efficiency

use core::cmp::min;

const MAX_POWER_STATES: usize = 7;  // C0-C6
const MAX_CPUS: usize = 64;

/// CPU power states (C-states)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    C0,  // Active
    C1,  // Halt
    C2,  // Stop-clock
    C3,  // Sleep
    C4,  // Deep sleep
    C5,  // Deeper sleep
    C6,  // Deepest sleep
}

impl PowerState {
    pub fn power_consumption_mw(&self) -> u32 {
        match self {
            PowerState::C0 => 100,
            PowerState::C1 => 80,
            PowerState::C2 => 60,
            PowerState::C3 => 40,
            PowerState::C4 => 20,
            PowerState::C5 => 10,
            PowerState::C6 => 1,
        }
    }

    pub fn wakeup_latency_us(&self) -> u32 {
        match self {
            PowerState::C0 => 0,
            PowerState::C1 => 1,
            PowerState::C2 => 10,
            PowerState::C3 => 100,
            PowerState::C4 => 1000,
            PowerState::C5 => 10000,
            PowerState::C6 => 100000,
        }
    }
}

/// Power modes (performance/balanced/power-saver)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerMode {
    Performance,
    Balanced,
    PowerSaver,
}

/// Frequency scaling P-state
#[derive(Debug, Clone, Copy)]
pub struct PState {
    pub frequency_mhz: u32,
    pub voltage_mv: u32,
    pub power_consumption_mw: u32,
}

impl PState {
    pub fn new(frequency_mhz: u32) -> Self {
        let voltage_mv = (1200 + (frequency_mhz / 100)).min(1500);
        let power_consumption_mw = frequency_mhz / 10;

        Self {
            frequency_mhz,
            voltage_mv,
            power_consumption_mw,
        }
    }
}

/// Thermal policy for throttling
#[derive(Debug, Clone, Copy)]
pub struct ThermalPolicy {
    pub current_temp_c: u32,
    pub critical_temp_c: u32,
    pub throttle_temp_c: u32,
    pub throttle_enabled: bool,
}

impl ThermalPolicy {
    pub fn new() -> Self {
        Self {
            current_temp_c: 45,
            critical_temp_c: 95,
            throttle_temp_c: 80,
            throttle_enabled: true,
        }
    }

    pub fn is_throttled(&self) -> bool {
        self.current_temp_c >= self.throttle_temp_c && self.throttle_enabled
    }

    pub fn is_critical(&self) -> bool {
        self.current_temp_c >= self.critical_temp_c
    }

    pub fn get_throttle_ratio(&self) -> u32 {
        if self.current_temp_c >= self.critical_temp_c {
            50  // 50% throttle
        } else if self.current_temp_c >= self.throttle_temp_c {
            25  // 25% throttle
        } else {
            0
        }
    }
}

/// Power budget enforcement
#[derive(Debug, Clone, Copy)]
pub struct PowerBudget {
    pub max_power_w: u32,
    pub current_power_w: u32,
    pub violations: u32,
}

impl PowerBudget {
    pub fn new(max_power_w: u32) -> Self {
        Self {
            max_power_w,
            current_power_w: 0,
            violations: 0,
        }
    }

    pub fn can_transition(&self, new_power_mw: u32) -> bool {
        (self.current_power_w * 1000 + new_power_mw as u32) <= (self.max_power_w * 1000)
    }

    pub fn is_violated(&self) -> bool {
        self.current_power_w > self.max_power_w
    }

    pub fn record_violation(&mut self) {
        self.violations = self.violations.saturating_add(1);
    }
}

/// Power management statistics
#[derive(Debug, Clone, Copy)]
pub struct PowerStats {
    pub total_energy_mj: u64,
    pub state_transitions: u32,
    pub avg_power_w: u32,
    pub idle_time_percent: u32,
}

impl PowerStats {
    pub fn new() -> Self {
        Self {
            total_energy_mj: 0,
            state_transitions: 0,
            avg_power_w: 50,
            idle_time_percent: 0,
        }
    }

    pub fn record_energy(&mut self, power_mw: u32, duration_us: u32) {
        let energy_muj = (power_mw as u64) * (duration_us as u64);
        self.total_energy_mj = self.total_energy_mj.saturating_add(energy_muj / 1000);
    }

    pub fn record_transition(&mut self) {
        self.state_transitions = self.state_transitions.saturating_add(1);
    }
}

/// Power optimizer
pub struct PowerOptimizer {
    current_state: PowerState,
    current_mode: PowerMode,
    pstates: [PState; 8],
    current_pstate_idx: u32,
    thermal: ThermalPolicy,
    budget: PowerBudget,
    stats: PowerStats,
    predictive_enabled: bool,
    clock_time_us: u64,
}

impl PowerOptimizer {
    pub fn new(mode: PowerMode) -> Self {
        let pstates = [
            PState::new(800),
            PState::new(1000),
            PState::new(1200),
            PState::new(1400),
            PState::new(1600),
            PState::new(1800),
            PState::new(2000),
            PState::new(2400),
        ];

        Self {
            current_state: PowerState::C0,
            current_mode: mode,
            pstates,
            current_pstate_idx: 2,
            thermal: ThermalPolicy::new(),
            budget: PowerBudget::new(100),
            stats: PowerStats::new(),
            predictive_enabled: true,
            clock_time_us: 0,
        }
    }

    pub fn transition_state(&mut self, target: PowerState) -> bool {
        if target == self.current_state {
            return true;
        }

        let current_power = self.pstates[self.current_pstate_idx as usize].power_consumption_mw;
        let target_power = target.power_consumption_mw();

        if !self.budget.can_transition(target_power) {
            self.budget.record_violation();
            return false;
        }

        self.current_state = target;
        self.stats.record_transition();
        true
    }

    pub fn scale_frequency(&mut self, target_frequency_mhz: u32) -> bool {
        // Find closest P-state
        let mut best_idx = 0;
        let mut best_diff = u32::MAX;

        for (i, pstate) in self.pstates.iter().enumerate() {
            let diff = if pstate.frequency_mhz >= target_frequency_mhz {
                pstate.frequency_mhz - target_frequency_mhz
            } else {
                target_frequency_mhz - pstate.frequency_mhz
            };

            if diff < best_diff {
                best_diff = diff;
                best_idx = i;
            }
        }

        let new_power = self.pstates[best_idx].power_consumption_mw;
        if !self.budget.can_transition(new_power) {
            return false;
        }

        self.current_pstate_idx = best_idx as u32;
        true
    }

    pub fn update_thermal(&mut self, current_temp_c: u32) {
        self.thermal.current_temp_c = current_temp_c;

        // Thermal throttling
        if self.thermal.is_critical() {
            let _ = self.transition_state(PowerState::C3);
        } else if self.thermal.is_throttled() {
            // Reduce frequency by 25%
            let current_freq = self.pstates[self.current_pstate_idx as usize].frequency_mhz;
            let reduced_freq = (current_freq * 3) / 4;
            let _ = self.scale_frequency(reduced_freq);
        }
    }

    pub fn select_idle_state(&mut self) -> PowerState {
        match self.current_mode {
            PowerMode::Performance => PowerState::C1,
            PowerMode::Balanced => PowerState::C3,
            PowerMode::PowerSaver => PowerState::C6,
        }
    }

    pub fn get_current_frequency(&self) -> u32 {
        self.pstates[self.current_pstate_idx as usize].frequency_mhz
    }

    pub fn get_current_power(&self) -> u32 {
        self.pstates[self.current_pstate_idx as usize].power_consumption_mw
    }

    pub fn get_stats(&self) -> PowerStats {
        self.stats
    }

    pub fn get_mode(&self) -> PowerMode {
        self.current_mode
    }

    pub fn get_state(&self) -> PowerState {
        self.current_state
    }

    pub fn set_mode(&mut self, mode: PowerMode) {
        self.current_mode = mode;
    }

    pub fn enable_predictive(&mut self, enabled: bool) {
        self.predictive_enabled = enabled;
    }

    pub fn advance_time(&mut self, delta_us: u64) {
        self.clock_time_us = self.clock_time_us.saturating_add(delta_us);
    }
}

// Bare metal compatible power management
// Tests run via shell interface: power [status|states|modes|thermal|budget|help]
