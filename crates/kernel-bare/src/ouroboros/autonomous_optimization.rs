//! Autonomous Optimization: AI-Driven Mutation Selection Without User Approval
//!
//! The Ouroboros Engine's intelligent mutation strategy powered by Bicameral Kernel
//! System 2 (reflective, slow, reasoning-based decision making). Selects mutations
//! to apply automatically based on profiling data, historical success patterns, and
//! confidence scoring.
//!
//! Phase 34, Task 2

/// Mutation confidence levels for autonomous decision-making
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(u8)]
pub enum ConfidenceLevel {
    VeryLow = 0,    // < 40%
    Low = 1,        // 40-60%
    Medium = 2,     // 60-75%
    High = 3,       // 75-90%
    VeryHigh = 4,   // > 90%
}

/// Decision reasoning from System 2 analysis
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DecisionReason {
    HistoricalSuccessRate = 0,  // Pattern matched in history
    ProfileHotspot = 1,          // Targets identified hotspot
    EnergyOptimization = 2,      // Expected power savings
    PerformanceGain = 3,         // Expected throughput improvement
    RegressionAvoidance = 4,     // Safety check passed
    Exploratory = 5,             // Intentional exploration (low-confidence)
}

/// System 2 confidence factors for mutation decisions
#[derive(Clone, Copy, Debug)]
pub struct ConfidenceFactors {
    /// Historical success rate (0-100%)
    pub historical_success_percent: u8,
    /// Similarity to past successful mutations (0-100%)
    pub pattern_match_confidence: u8,
    /// Profiling data quality (0-100%)
    pub profile_data_quality: u8,
    /// System stability score (0-100%)
    pub system_stability: u8,
    /// Regression safety score (0-100%)
    pub regression_safety: u8,
}

impl ConfidenceFactors {
    /// Create new confidence factors
    pub const fn new(
        historical: u8,
        pattern: u8,
        profile: u8,
        stability: u8,
        safety: u8,
    ) -> Self {
        ConfidenceFactors {
            historical_success_percent: historical,
            pattern_match_confidence: pattern,
            profile_data_quality: profile,
            system_stability: stability,
            regression_safety: safety,
        }
    }

    /// Calculate weighted confidence (System 2 reasoning)
    pub fn calculate_confidence(&self) -> u8 {
        // Weights: historical (30%), pattern (25%), profile (20%), stability (15%), safety (10%)
        let weighted = ((self.historical_success_percent as u32) * 30
            + (self.pattern_match_confidence as u32) * 25
            + (self.profile_data_quality as u32) * 20
            + (self.system_stability as u32) * 15
            + (self.regression_safety as u32) * 10) / 100;
        weighted as u8
    }

    /// Get confidence level
    pub fn confidence_level(&self) -> ConfidenceLevel {
        let confidence = self.calculate_confidence();
        match confidence {
            0..=39 => ConfidenceLevel::VeryLow,
            40..=59 => ConfidenceLevel::Low,
            60..=74 => ConfidenceLevel::Medium,
            75..=89 => ConfidenceLevel::High,
            _ => ConfidenceLevel::VeryHigh,
        }
    }
}

/// Historical mutation pattern
#[derive(Clone, Copy, Debug)]
pub struct MutationPattern {
    /// Pattern ID
    pub id: u32,
    /// Target function hash
    pub target_hash: u64,
    /// Mutation type ID
    pub mutation_type: u8,
    /// Success rate (0-100%)
    pub success_rate: u8,
    /// Application count
    pub applied_count: u32,
    /// Last applied timestamp (ms)
    pub last_applied_ms: u64,
}

impl MutationPattern {
    /// Create new mutation pattern
    pub const fn new(id: u32, target_hash: u64, mutation_type: u8) -> Self {
        MutationPattern {
            id,
            target_hash,
            mutation_type,
            success_rate: 0,
            applied_count: 0,
            last_applied_ms: 0,
        }
    }

    /// Update success rate based on result
    pub fn update_success(&mut self, succeeded: bool) {
        self.applied_count += 1;
        if succeeded {
            // Moving average: new_rate = (old_rate * (count-1) + 100) / count
            if self.success_rate == 0 && self.applied_count == 1 {
                self.success_rate = 100;
            } else {
                let new_rate =
                    (self.success_rate as u32 * (self.applied_count - 1) + 100) / self.applied_count;
                self.success_rate = new_rate as u8;
            }
        } else {
            // Moving average for failure
            let new_rate = (self.success_rate as u32 * (self.applied_count - 1)) / self.applied_count;
            self.success_rate = new_rate as u8;
        }
    }

    /// Is this pattern mature (enough samples)
    pub fn is_mature(&self) -> bool {
        self.applied_count >= 5
    }
}

/// System 2 mutation candidate (with reasoning)
#[derive(Clone, Copy, Debug)]
pub struct AutoMutationCandidate {
    /// Candidate ID
    pub id: u32,
    /// Target function hash
    pub target_hash: u64,
    /// Mutation type
    pub mutation_type: u8,
    /// Confidence factors
    pub confidence: u8,
    /// Primary reason for selection
    pub reason: DecisionReason,
    /// System 2 reasoning metadata
    pub reasoning_flags: u8,  // Bit 0: has_pattern, Bit 1: hotspot_targeted, Bit 2: safe
    /// Expected improvement (percent)
    pub expected_improvement: u8,
}

impl AutoMutationCandidate {
    /// Create new auto mutation candidate
    pub const fn new(id: u32, target_hash: u64, mutation_type: u8, confidence: u8) -> Self {
        AutoMutationCandidate {
            id,
            target_hash,
            mutation_type,
            confidence,
            reason: DecisionReason::Exploratory,
            reasoning_flags: 0,
            expected_improvement: 0,
        }
    }

    /// Should apply automatically (confidence threshold)
    pub fn should_apply_auto(&self, threshold: u8) -> bool {
        self.confidence >= threshold
    }

    /// Set has historical pattern
    pub fn set_has_pattern(&mut self) {
        self.reasoning_flags |= 0x01;
    }

    /// Set hotspot targeted
    pub fn set_hotspot_targeted(&mut self) {
        self.reasoning_flags |= 0x02;
    }

    /// Set safety verified
    pub fn set_safety_verified(&mut self) {
        self.reasoning_flags |= 0x04;
    }

    /// Check if has pattern
    pub fn has_pattern(&self) -> bool {
        (self.reasoning_flags & 0x01) != 0
    }

    /// Check if targets hotspot
    pub fn targets_hotspot(&self) -> bool {
        (self.reasoning_flags & 0x02) != 0
    }

    /// Check if safety verified
    pub fn is_safety_verified(&self) -> bool {
        (self.reasoning_flags & 0x04) != 0
    }
}

/// System 2 decision from autonomous optimizer
#[derive(Clone, Copy, Debug)]
pub struct AutoOptimizationDecision {
    /// Decision ID
    pub id: u32,
    /// Candidate ID selected
    pub candidate_id: u32,
    /// Final confidence score
    pub confidence: u8,
    /// Apply automatically (true) or queue for review (false)
    pub apply_auto: bool,
    /// Decision timestamp (ms)
    pub decided_at_ms: u64,
}

impl AutoOptimizationDecision {
    /// Create new decision
    pub const fn new(id: u32, candidate_id: u32, confidence: u8) -> Self {
        AutoOptimizationDecision {
            id,
            candidate_id,
            confidence,
            apply_auto: false,
            decided_at_ms: 0,
        }
    }

    /// Mark for auto-application
    pub fn set_auto_apply(&mut self) {
        self.apply_auto = true;
    }
}

/// Autonomous Optimizer (System 2 reasoning engine)
pub struct AutonomousOptimizer {
    /// Mutation patterns (max 100)
    patterns: [Option<MutationPattern>; 100],
    /// Auto mutation candidates (max 50)
    candidates: [Option<AutoMutationCandidate>; 50],
    /// Recent decisions (max 30)
    decisions: [Option<AutoOptimizationDecision>; 30],
    /// Auto-apply confidence threshold (0-100)
    auto_apply_threshold: u8,
    /// Active optimization flag
    optimization_active: bool,
    /// Statistics
    total_decisions: u32,
    auto_applied_count: u32,
    successful_auto_applies: u32,
}

impl AutonomousOptimizer {
    /// Create new autonomous optimizer
    pub const fn new() -> Self {
        AutonomousOptimizer {
            patterns: [None; 100],
            candidates: [None; 50],
            decisions: [None; 30],
            auto_apply_threshold: 75,  // Default: apply if confidence >= 75%
            optimization_active: false,
            total_decisions: 0,
            auto_applied_count: 0,
            successful_auto_applies: 0,
        }
    }

    /// Register mutation pattern from history
    pub fn register_pattern(&mut self, pattern: MutationPattern) -> bool {
        for slot in &mut self.patterns {
            if slot.is_none() {
                *slot = Some(pattern);
                return true;
            }
        }
        false
    }

    /// Submit mutation candidate for System 2 analysis
    pub fn submit_candidate(&mut self, candidate: AutoMutationCandidate) -> bool {
        for slot in &mut self.candidates {
            if slot.is_none() {
                *slot = Some(candidate);
                return true;
            }
        }
        false
    }

    /// Analyze candidate with System 2 reasoning
    pub fn analyze_candidate(&mut self, candidate_id: u32, factors: ConfidenceFactors) -> bool {
        for slot in &mut self.candidates {
            if let Some(ref mut cand) = slot {
                if cand.id == candidate_id {
                    cand.confidence = factors.calculate_confidence();

                    // Apply System 2 reasoning
                    if cand.confidence >= 75 {
                        cand.set_safety_verified();
                    }

                    // Check historical pattern
                    for pattern in &self.patterns {
                        if let Some(p) = pattern {
                            if p.target_hash == cand.target_hash && p.success_rate >= 80 {
                                cand.set_has_pattern();
                                cand.reason = DecisionReason::HistoricalSuccessRate;
                                break;
                            }
                        }
                    }

                    return true;
                }
            }
        }
        false
    }

    /// Make decision on candidate (System 2 output)
    pub fn decide(&mut self, candidate_id: u32) -> Option<AutoOptimizationDecision> {
        let mut decision = None;

        for slot in &mut self.candidates {
            if let Some(cand) = slot {
                if cand.id == candidate_id {
                    let mut dec = AutoOptimizationDecision::new(self.total_decisions, candidate_id, cand.confidence);

                    if cand.should_apply_auto(self.auto_apply_threshold) {
                        dec.set_auto_apply();
                        self.auto_applied_count += 1;
                    }

                    self.total_decisions += 1;
                    decision = Some(dec);
                    break;
                }
            }
        }

        if let Some(dec) = decision {
            // Store decision in history
            for slot in &mut self.decisions {
                if slot.is_none() {
                    *slot = Some(dec);
                    break;
                }
            }
        }

        decision
    }

    /// Update pattern with result
    pub fn update_pattern(&mut self, pattern_id: u32, succeeded: bool) -> bool {
        for slot in &mut self.patterns {
            if let Some(ref mut p) = slot {
                if p.id == pattern_id {
                    p.update_success(succeeded);
                    if succeeded {
                        self.successful_auto_applies += 1;
                    }
                    return true;
                }
            }
        }
        false
    }

    /// Get best candidate (highest confidence)
    pub fn best_candidate(&self) -> Option<AutoMutationCandidate> {
        let mut best = None;
        let mut best_confidence = 0;

        for slot in &self.candidates {
            if let Some(cand) = slot {
                if cand.confidence > best_confidence {
                    best_confidence = cand.confidence;
                    best = Some(*cand);
                }
            }
        }

        best
    }

    /// Set auto-apply threshold
    pub fn set_auto_apply_threshold(&mut self, threshold: u8) {
        self.auto_apply_threshold = threshold.min(100);
    }

    /// Start optimization cycle
    pub fn start_optimization(&mut self) {
        self.optimization_active = true;
    }

    /// End optimization cycle
    pub fn end_optimization(&mut self) {
        self.optimization_active = false;
    }

    /// Is optimization active
    pub fn is_active(&self) -> bool {
        self.optimization_active
    }

    /// Get total decisions made
    pub fn total_decisions(&self) -> u32 {
        self.total_decisions
    }

    /// Get auto-applied count
    pub fn auto_applied(&self) -> u32 {
        self.auto_applied_count
    }

    /// Get successful auto-applies
    pub fn successful_auto_applies(&self) -> u32 {
        self.successful_auto_applies
    }

    /// Get auto-apply success rate
    pub fn auto_apply_success_rate(&self) -> u8 {
        if self.auto_applied_count == 0 {
            0
        } else {
            ((self.successful_auto_applies as u32 * 100) / self.auto_applied_count) as u8
        }
    }

    /// Get pattern by ID
    pub fn get_pattern(&self, pattern_id: u32) -> Option<MutationPattern> {
        for slot in &self.patterns {
            if let Some(p) = slot {
                if p.id == pattern_id {
                    return Some(*p);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confidence_levels() {
        assert_eq!(ConfidenceLevel::VeryLow as u8, 0);
        assert_eq!(ConfidenceLevel::High as u8, 3);
        assert_eq!(ConfidenceLevel::VeryHigh as u8, 4);
    }

    #[test]
    fn test_confidence_factors_new() {
        let factors = ConfidenceFactors::new(85, 80, 75, 90, 70);
        assert_eq!(factors.historical_success_percent, 85);
        assert_eq!(factors.pattern_match_confidence, 80);
    }

    #[test]
    fn test_confidence_calculation() {
        let factors = ConfidenceFactors::new(80, 80, 80, 80, 80);
        let confidence = factors.calculate_confidence();
        assert_eq!(confidence, 80);
    }

    #[test]
    fn test_confidence_level_mapping() {
        let low = ConfidenceFactors::new(50, 50, 50, 50, 50);
        assert_eq!(low.confidence_level(), ConfidenceLevel::Medium);

        let high = ConfidenceFactors::new(90, 90, 90, 90, 90);
        assert_eq!(high.confidence_level(), ConfidenceLevel::VeryHigh);

        let very_low = ConfidenceFactors::new(20, 20, 20, 20, 20);
        assert_eq!(very_low.confidence_level(), ConfidenceLevel::VeryLow);
    }

    #[test]
    fn test_mutation_pattern_creation() {
        let pattern = MutationPattern::new(1, 0x1234567890abcdef, 1);
        assert_eq!(pattern.id, 1);
        assert_eq!(pattern.applied_count, 0);
        assert_eq!(pattern.success_rate, 0);
    }

    #[test]
    fn test_mutation_pattern_success_update() {
        let mut pattern = MutationPattern::new(1, 0x1234567890abcdef, 1);
        pattern.update_success(true);
        assert_eq!(pattern.applied_count, 1);
        assert_eq!(pattern.success_rate, 100);
    }

    #[test]
    fn test_mutation_pattern_success_rate_moving_average() {
        let mut pattern = MutationPattern::new(1, 0x1234567890abcdef, 1);
        pattern.update_success(true);
        pattern.update_success(true);
        pattern.update_success(false);
        // (100 * 2 + 0) / 3 = 66
        assert!(pattern.success_rate >= 66 && pattern.success_rate <= 67);
    }

    #[test]
    fn test_mutation_pattern_maturity() {
        let mut pattern = MutationPattern::new(1, 0x1234567890abcdef, 1);
        assert!(!pattern.is_mature());

        for _ in 0..5 {
            pattern.update_success(true);
        }
        assert!(pattern.is_mature());
    }

    #[test]
    fn test_auto_mutation_candidate_creation() {
        let candidate = AutoMutationCandidate::new(1, 0x1234567890abcdef, 1, 80);
        assert_eq!(candidate.id, 1);
        assert_eq!(candidate.confidence, 80);
        assert_eq!(candidate.reasoning_flags, 0);
    }

    #[test]
    fn test_auto_mutation_candidate_reasoning_flags() {
        let mut candidate = AutoMutationCandidate::new(1, 0x1234567890abcdef, 1, 80);
        assert!(!candidate.has_pattern());

        candidate.set_has_pattern();
        assert!(candidate.has_pattern());

        candidate.set_hotspot_targeted();
        assert!(candidate.targets_hotspot());

        candidate.set_safety_verified();
        assert!(candidate.is_safety_verified());
    }

    #[test]
    fn test_auto_mutation_candidate_should_apply() {
        let candidate = AutoMutationCandidate::new(1, 0x1234567890abcdef, 1, 80);
        assert!(candidate.should_apply_auto(75));
        assert!(!candidate.should_apply_auto(90));
    }

    #[test]
    fn test_auto_optimization_decision_creation() {
        let decision = AutoOptimizationDecision::new(1, 1, 85);
        assert_eq!(decision.id, 1);
        assert_eq!(decision.confidence, 85);
        assert!(!decision.apply_auto);
    }

    #[test]
    fn test_auto_optimization_decision_set_auto_apply() {
        let mut decision = AutoOptimizationDecision::new(1, 1, 85);
        assert!(!decision.apply_auto);
        decision.set_auto_apply();
        assert!(decision.apply_auto);
    }

    #[test]
    fn test_autonomous_optimizer_creation() {
        let optimizer = AutonomousOptimizer::new();
        assert_eq!(optimizer.auto_apply_threshold, 75);
        assert!(!optimizer.is_active());
        assert_eq!(optimizer.total_decisions(), 0);
    }

    #[test]
    fn test_autonomous_optimizer_register_pattern() {
        let mut optimizer = AutonomousOptimizer::new();
        let pattern = MutationPattern::new(1, 0x1234567890abcdef, 1);
        assert!(optimizer.register_pattern(pattern));
    }

    #[test]
    fn test_autonomous_optimizer_submit_candidate() {
        let mut optimizer = AutonomousOptimizer::new();
        let candidate = AutoMutationCandidate::new(1, 0x1234567890abcdef, 1, 80);
        assert!(optimizer.submit_candidate(candidate));
    }

    #[test]
    fn test_autonomous_optimizer_analyze_candidate() {
        let mut optimizer = AutonomousOptimizer::new();
        let candidate = AutoMutationCandidate::new(1, 0x1234567890abcdef, 1, 50);
        optimizer.submit_candidate(candidate);

        let factors = ConfidenceFactors::new(80, 80, 80, 80, 80);
        assert!(optimizer.analyze_candidate(1, factors));
    }

    #[test]
    fn test_autonomous_optimizer_decide() {
        let mut optimizer = AutonomousOptimizer::new();
        let candidate = AutoMutationCandidate::new(1, 0x1234567890abcdef, 1, 80);
        optimizer.submit_candidate(candidate);

        let decision = optimizer.decide(1);
        assert!(decision.is_some());
        assert!(decision.unwrap().apply_auto);
    }

    #[test]
    fn test_autonomous_optimizer_update_pattern() {
        let mut optimizer = AutonomousOptimizer::new();
        let pattern = MutationPattern::new(1, 0x1234567890abcdef, 1);
        optimizer.register_pattern(pattern);

        assert!(optimizer.update_pattern(1, true));
    }

    #[test]
    fn test_autonomous_optimizer_best_candidate() {
        let mut optimizer = AutonomousOptimizer::new();
        optimizer.submit_candidate(AutoMutationCandidate::new(1, 0x1111111111111111, 1, 60));
        optimizer.submit_candidate(AutoMutationCandidate::new(2, 0x2222222222222222, 2, 85));
        optimizer.submit_candidate(AutoMutationCandidate::new(3, 0x3333333333333333, 3, 70));

        let best = optimizer.best_candidate();
        assert!(best.is_some());
        assert_eq!(best.unwrap().id, 2);
        assert_eq!(best.unwrap().confidence, 85);
    }

    #[test]
    fn test_autonomous_optimizer_set_threshold() {
        let mut optimizer = AutonomousOptimizer::new();
        assert_eq!(optimizer.auto_apply_threshold, 75);

        optimizer.set_auto_apply_threshold(90);
        assert_eq!(optimizer.auto_apply_threshold, 90);

        optimizer.set_auto_apply_threshold(200);  // Should clamp to 100
        assert_eq!(optimizer.auto_apply_threshold, 100);
    }

    #[test]
    fn test_autonomous_optimizer_lifecycle() {
        let mut optimizer = AutonomousOptimizer::new();
        assert!(!optimizer.is_active());

        optimizer.start_optimization();
        assert!(optimizer.is_active());

        optimizer.end_optimization();
        assert!(!optimizer.is_active());
    }

    #[test]
    fn test_autonomous_optimizer_statistics() {
        let mut optimizer = AutonomousOptimizer::new();
        let candidate = AutoMutationCandidate::new(1, 0x1234567890abcdef, 1, 80);
        optimizer.submit_candidate(candidate);

        let decision = optimizer.decide(1);
        assert!(decision.is_some());
        assert_eq!(optimizer.total_decisions(), 1);
        assert_eq!(optimizer.auto_applied(), 1);
    }

    #[test]
    fn test_autonomous_optimizer_success_rate() {
        let mut optimizer = AutonomousOptimizer::new();
        let mut pattern = MutationPattern::new(1, 0x1234567890abcdef, 1);
        pattern.applied_count = 3;
        pattern.success_rate = 66;
        optimizer.register_pattern(pattern);

        optimizer.auto_applied_count = 3;
        optimizer.successful_auto_applies = 2;

        let rate = optimizer.auto_apply_success_rate();
        assert!(rate >= 66 && rate <= 67);  // 2/3 = 66.67%
    }

    #[test]
    fn test_autonomous_optimizer_max_candidates() {
        let mut optimizer = AutonomousOptimizer::new();

        for i in 0..50 {
            let candidate = AutoMutationCandidate::new(i, 0x1000 + i as u64, (i % 5) as u8, 70 + (i % 20) as u8);
            assert!(optimizer.submit_candidate(candidate));
        }

        // 51st should fail
        let candidate = AutoMutationCandidate::new(50, 0x2000, 1, 70);
        assert!(!optimizer.submit_candidate(candidate));
    }

    #[test]
    fn test_decision_reason_enum() {
        assert_eq!(DecisionReason::HistoricalSuccessRate as u8, 0);
        assert_eq!(DecisionReason::ProfileHotspot as u8, 1);
        assert_eq!(DecisionReason::Exploratory as u8, 5);
    }

    #[test]
    fn test_confidence_factors_weighted_calculation() {
        // Historical: 100, Pattern: 100, Profile: 100, Stability: 100, Safety: 100
        // = (100*30 + 100*25 + 100*20 + 100*15 + 100*10) / 100 = 100
        let factors = ConfidenceFactors::new(100, 100, 100, 100, 100);
        assert_eq!(factors.calculate_confidence(), 100);
    }

    #[test]
    fn test_autonomous_optimizer_get_pattern() {
        let mut optimizer = AutonomousOptimizer::new();
        let pattern = MutationPattern::new(1, 0x1234567890abcdef, 2);
        optimizer.register_pattern(pattern);

        let retrieved = optimizer.get_pattern(1);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, 1);
        assert_eq!(retrieved.unwrap().mutation_type, 2);
    }

    #[test]
    fn test_pattern_with_historical_reasoning() {
        let mut optimizer = AutonomousOptimizer::new();

        let mut pattern = MutationPattern::new(1, 0x1234567890abcdef, 1);
        pattern.success_rate = 90;
        optimizer.register_pattern(pattern);

        let mut candidate = AutoMutationCandidate::new(1, 0x1234567890abcdef, 1, 75);
        optimizer.submit_candidate(candidate);

        let factors = ConfidenceFactors::new(80, 80, 80, 80, 80);
        optimizer.analyze_candidate(1, factors);

        // After analysis, candidate should have pattern flag
        for slot in &optimizer.candidates {
            if let Some(cand) = slot {
                if cand.id == 1 {
                    assert!(cand.has_pattern());
                }
            }
        }
    }
}
