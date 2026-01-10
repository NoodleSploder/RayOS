//! Feedback Loop System: Learning from Mutation Outcomes
//!
//! Continuously learns from mutation successes and failures to adapt and improve
//! the mutation strategy. Uses historical feedback to guide future mutations and
//! identify patterns that lead to performance improvements.
//!
//! Phase 34, Task 4

/// Feedback metric type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FeedbackMetric {
    Throughput = 0,         // Instructions/second
    Latency = 1,            // Execution time
    EnergyEfficiency = 2,   // Instructions per joule
    MemoryUsage = 3,        // Peak memory
    CacheEfficiency = 4,    // Cache hit rate
    CompilationTime = 5,    // Time to apply mutation
}

/// Mutation outcome classification
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum OutcomeType {
    GreatSuccess = 0,   // > 10% improvement
    Success = 1,        // 3-10% improvement
    Neutral = 2,        // -2% to 3%
    MinorFailure = 3,   // -2% to -10%
    CriticalFailure = 4,// < -10%
}

impl OutcomeType {
    /// Get outcome score (-100 to +100)
    pub fn score(&self) -> i8 {
        match self {
            OutcomeType::GreatSuccess => 100,
            OutcomeType::Success => 50,
            OutcomeType::Neutral => 0,
            OutcomeType::MinorFailure => -25,
            OutcomeType::CriticalFailure => -100,
        }
    }

    /// Should learn from this outcome
    pub fn should_learn(&self) -> bool {
        !matches!(self, OutcomeType::CriticalFailure)
    }
}

/// Single feedback entry (mutation outcome)
#[derive(Clone, Copy, Debug)]
pub struct FeedbackEntry {
    /// Entry ID
    pub id: u32,
    /// Target function hash
    pub target_hash: u64,
    /// Mutation type applied
    pub mutation_type: u8,
    /// Metric improved
    pub metric: FeedbackMetric,
    /// Before value
    pub value_before: u32,
    /// After value
    pub value_after: u32,
    /// Outcome classification
    pub outcome: OutcomeType,
    /// Timestamp (ms)
    pub timestamp_ms: u64,
}

impl FeedbackEntry {
    /// Create new feedback entry
    pub const fn new(
        id: u32,
        target_hash: u64,
        mutation_type: u8,
        metric: FeedbackMetric,
        before: u32,
        after: u32,
    ) -> Self {
        let improvement_percent = if before == 0 {
            0
        } else {
            ((after as i32 - before as i32) * 100) / before as i32
        };

        let outcome = match improvement_percent {
            i if i > 10 => OutcomeType::GreatSuccess,
            i if i > 3 => OutcomeType::Success,
            i if i >= -2 => OutcomeType::Neutral,
            i if i >= -10 => OutcomeType::MinorFailure,
            _ => OutcomeType::CriticalFailure,
        };

        FeedbackEntry {
            id,
            target_hash,
            mutation_type,
            metric,
            value_before: before,
            value_after: after,
            outcome,
            timestamp_ms: 0,
        }
    }

    /// Calculate improvement percent
    pub fn improvement_percent(&self) -> i8 {
        if self.value_before == 0 {
            return 0;
        }
        let improvement = ((self.value_after as i32 - self.value_before as i32) * 100)
            / self.value_before as i32;
        improvement.max(-100).min(100) as i8
    }

    /// Is positive outcome
    pub fn is_positive(&self) -> bool {
        matches!(self.outcome, OutcomeType::GreatSuccess | OutcomeType::Success)
    }
}

/// Learned pattern from successful mutations
#[derive(Clone, Copy, Debug)]
pub struct LearningPattern {
    /// Pattern ID
    pub id: u32,
    /// Target function hash
    pub target_hash: u64,
    /// Mutation type that was successful
    pub mutation_type: u8,
    /// Metric it improved
    pub metric: FeedbackMetric,
    /// Success rate (0-100%)
    pub success_rate: u8,
    /// Average improvement (percent)
    pub avg_improvement: i8,
    /// Number of times applied
    pub application_count: u32,
    /// Number of successes
    pub success_count: u32,
}

impl LearningPattern {
    /// Create new learning pattern
    pub const fn new(id: u32, target_hash: u64, mutation_type: u8, metric: FeedbackMetric) -> Self {
        LearningPattern {
            id,
            target_hash,
            mutation_type,
            metric,
            success_rate: 0,
            avg_improvement: 0,
            application_count: 0,
            success_count: 0,
        }
    }

    /// Record successful application
    pub fn record_success(&mut self, improvement: i8) {
        self.application_count += 1;
        self.success_count += 1;

        // Update moving average
        if self.success_count == 1 {
            self.avg_improvement = improvement;
        } else {
            let new_avg = (self.avg_improvement as i32 * (self.success_count - 1) as i32
                + improvement as i32)
                / self.success_count as i32;
            self.avg_improvement = new_avg as i8;
        }

        // Update success rate
        self.success_rate = ((self.success_count as u32 * 100) / self.application_count) as u8;
    }

    /// Record failed application
    pub fn record_failure(&mut self) {
        self.application_count += 1;
        self.success_rate = ((self.success_count as u32 * 100) / self.application_count) as u8;
    }

    /// Is mature pattern (enough data)
    pub fn is_mature(&self) -> bool {
        self.application_count >= 5
    }

    /// Pattern reliability score (0-100)
    pub fn reliability_score(&self) -> u8 {
        if self.application_count < 3 {
            return 0;  // Immature pattern
        }
        // Score = 50% success_rate + 50% avg_improvement (normalized)
        let improvement_score = ((self.avg_improvement + 50).max(0).min(100)) as u8;
        ((self.success_rate as u32 + improvement_score as u32) / 2) as u8
    }
}

/// Adaptive strategy based on feedback
#[derive(Clone, Copy, Debug)]
pub struct AdaptiveStrategy {
    /// Strategy ID
    pub id: u32,
    /// Target function hash
    pub target_hash: u64,
    /// Preferred mutation type (based on learning)
    pub preferred_mutation: u8,
    /// Preferred metric to optimize
    pub preferred_metric: FeedbackMetric,
    /// Strategy confidence (0-100)
    pub confidence: u8,
    /// Times this strategy was applied
    pub applied_count: u32,
    /// Times this strategy succeeded
    pub success_count: u32,
}

impl AdaptiveStrategy {
    /// Create new adaptive strategy
    pub const fn new(id: u32, target_hash: u64) -> Self {
        AdaptiveStrategy {
            id,
            target_hash,
            preferred_mutation: 0,
            preferred_metric: FeedbackMetric::Throughput,
            confidence: 0,
            applied_count: 0,
            success_count: 0,
        }
    }

    /// Update with outcome
    pub fn update(&mut self, succeeded: bool) {
        self.applied_count += 1;
        if succeeded {
            self.success_count += 1;
        }
        // Confidence = success_rate capped at 95%
        self.confidence = ((self.success_count as u32 * 100) / self.applied_count)
            .min(95)
            .max(0) as u8;
    }

    /// Set preferred mutation and metric
    pub fn set_preference(&mut self, mutation: u8, metric: FeedbackMetric) {
        self.preferred_mutation = mutation;
        self.preferred_metric = metric;
    }

    /// Get strategy effectiveness (0-100)
    pub fn effectiveness(&self) -> u8 {
        if self.applied_count < 2 {
            return 0;
        }
        ((self.success_count as u32 * 100) / self.applied_count) as u8
    }
}

/// Feedback Loop Controller
pub struct FeedbackLoop {
    /// Feedback history (max 100)
    feedback_entries: [Option<FeedbackEntry>; 100],
    /// Learning patterns (max 50)
    learning_patterns: [Option<LearningPattern>; 50],
    /// Adaptive strategies (max 30)
    adaptive_strategies: [Option<AdaptiveStrategy>; 30],
    /// Total feedback entries
    total_entries: u32,
    /// Learning enabled
    learning_enabled: bool,
}

impl FeedbackLoop {
    /// Create new feedback loop
    pub const fn new() -> Self {
        FeedbackLoop {
            feedback_entries: [None; 100],
            learning_patterns: [None; 50],
            adaptive_strategies: [None; 30],
            total_entries: 0,
            learning_enabled: true,
        }
    }

    /// Submit feedback entry
    pub fn submit_feedback(&mut self, entry: FeedbackEntry) -> bool {
        if !self.learning_enabled {
            return false;
        }

        for slot in &mut self.feedback_entries {
            if slot.is_none() {
                *slot = Some(entry);
                self.total_entries += 1;
                return true;
            }
        }
        false
    }

    /// Learn from feedback entry
    pub fn learn_from_entry(&mut self, entry: FeedbackEntry) -> bool {
        if !entry.outcome.should_learn() {
            return false;
        }

        // Find or create learning pattern
        for slot in &mut self.learning_patterns {
            if let Some(ref mut pattern) = slot {
                if pattern.target_hash == entry.target_hash
                    && pattern.mutation_type == entry.mutation_type
                    && pattern.metric == entry.metric
                {
                    if entry.is_positive() {
                        pattern.record_success(entry.improvement_percent());
                    } else {
                        pattern.record_failure();
                    }
                    return true;
                }
            }
        }

        // Create new pattern
        let mut pattern = LearningPattern::new(
            self.total_entries,
            entry.target_hash,
            entry.mutation_type,
            entry.metric,
        );

        if entry.is_positive() {
            pattern.record_success(entry.improvement_percent());
        } else {
            pattern.record_failure();
        }

        // Store pattern
        for slot in &mut self.learning_patterns {
            if slot.is_none() {
                *slot = Some(pattern);
                return true;
            }
        }

        false
    }

    /// Create or update adaptive strategy
    pub fn update_strategy(&mut self, target_hash: u64, succeeded: bool) -> bool {
        for slot in &mut self.adaptive_strategies {
            if let Some(ref mut strategy) = slot {
                if strategy.target_hash == target_hash {
                    strategy.update(succeeded);
                    return true;
                }
            }
        }

        // Create new strategy
        let mut strategy = AdaptiveStrategy::new(self.total_entries, target_hash);
        strategy.update(succeeded);

        for slot in &mut self.adaptive_strategies {
            if slot.is_none() {
                *slot = Some(strategy);
                return true;
            }
        }

        false
    }

    /// Get best learning pattern for target
    pub fn best_pattern_for_target(&self, target_hash: u64) -> Option<LearningPattern> {
        let mut best = None;
        let mut best_reliability = 0;

        for slot in &self.learning_patterns {
            if let Some(pattern) = slot {
                if pattern.target_hash == target_hash && pattern.is_mature() {
                    let reliability = pattern.reliability_score();
                    if reliability > best_reliability {
                        best_reliability = reliability;
                        best = Some(*pattern);
                    }
                }
            }
        }

        best
    }

    /// Get strategy for target
    pub fn get_strategy(&self, target_hash: u64) -> Option<AdaptiveStrategy> {
        for slot in &self.adaptive_strategies {
            if let Some(strategy) = slot {
                if strategy.target_hash == target_hash {
                    return Some(*strategy);
                }
            }
        }
        None
    }

    /// Get success rate for mutation type on target
    pub fn success_rate(&self, target_hash: u64, mutation_type: u8) -> u8 {
        let mut total = 0;
        let mut successes = 0;

        for slot in &self.feedback_entries {
            if let Some(entry) = slot {
                if entry.target_hash == target_hash && entry.mutation_type == mutation_type {
                    total += 1;
                    if entry.is_positive() {
                        successes += 1;
                    }
                }
            }
        }

        if total == 0 {
            0
        } else {
            ((successes as u32 * 100) / total as u32) as u8
        }
    }

    /// Get top patterns by reliability
    pub fn top_patterns(&self, _n: usize) -> [Option<LearningPattern>; 50] {
        let mut patterns = self.learning_patterns;

        // Simple bubble sort by reliability score
        for i in 0..50 {
            for j in 0..49 - i {
                let score_j = patterns[j].map(|p| p.reliability_score()).unwrap_or(0);
                let score_jp1 = patterns[j + 1].map(|p| p.reliability_score()).unwrap_or(0);

                if score_j < score_jp1 {
                    patterns.swap(j, j + 1);
                }
            }
        }

        patterns
    }

    /// Enable/disable learning
    pub fn set_learning_enabled(&mut self, enabled: bool) {
        self.learning_enabled = enabled;
    }

    /// Get total feedback collected
    pub fn total_feedback(&self) -> u32 {
        self.total_entries
    }

    /// Get learning statistics
    pub fn learning_statistics(&self) -> (u32, u32, u8) {
        let mut total_patterns = 0;
        let mut mature_patterns = 0;
        let mut avg_reliability = 0u32;

        for slot in &self.learning_patterns {
            if let Some(pattern) = slot {
                total_patterns += 1;
                if pattern.is_mature() {
                    mature_patterns += 1;
                    avg_reliability += pattern.reliability_score() as u32;
                }
            }
        }

        let avg = if mature_patterns > 0 {
            (avg_reliability / mature_patterns as u32) as u8
        } else {
            0
        };

        (total_patterns, mature_patterns, avg)
    }

    /// Clear old feedback (keep last N)
    pub fn prune_old_feedback(&mut self, keep_last: usize) {
        if self.total_entries <= keep_last as u32 {
            return;
        }

        let mut count = 0;
        let skip = (self.total_entries - keep_last as u32) as usize;

        for slot in &mut self.feedback_entries {
            if slot.is_some() {
                if count < skip {
                    *slot = None;
                }
                count += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feedback_metric_enum() {
        assert_eq!(FeedbackMetric::Throughput as u8, 0);
        assert_eq!(FeedbackMetric::EnergyEfficiency as u8, 2);
        assert_eq!(FeedbackMetric::CompilationTime as u8, 5);
    }

    #[test]
    fn test_outcome_type_enum() {
        assert_eq!(OutcomeType::GreatSuccess as u8, 0);
        assert_eq!(OutcomeType::CriticalFailure as u8, 4);
    }

    #[test]
    fn test_outcome_type_score() {
        assert_eq!(OutcomeType::GreatSuccess.score(), 100);
        assert_eq!(OutcomeType::Success.score(), 50);
        assert_eq!(OutcomeType::Neutral.score(), 0);
        assert_eq!(OutcomeType::MinorFailure.score(), -25);
        assert_eq!(OutcomeType::CriticalFailure.score(), -100);
    }

    #[test]
    fn test_outcome_type_should_learn() {
        assert!(OutcomeType::GreatSuccess.should_learn());
        assert!(OutcomeType::Neutral.should_learn());
        assert!(!OutcomeType::CriticalFailure.should_learn());
    }

    #[test]
    fn test_feedback_entry_creation() {
        let entry = FeedbackEntry::new(
            1,
            0x1234567890abcdef,
            1,
            FeedbackMetric::Throughput,
            1000,
            1100,
        );
        assert_eq!(entry.id, 1);
        assert_eq!(entry.outcome, OutcomeType::GreatSuccess);  // 10% improvement
    }

    #[test]
    fn test_feedback_entry_improvement_percent() {
        let entry = FeedbackEntry::new(
            1,
            0x1234567890abcdef,
            1,
            FeedbackMetric::Throughput,
            1000,
            1050,
        );
        let improvement = entry.improvement_percent();
        assert!(improvement >= 4 && improvement <= 6);  // 5%
    }

    #[test]
    fn test_feedback_entry_is_positive() {
        let success = FeedbackEntry::new(
            1,
            0x1234567890abcdef,
            1,
            FeedbackMetric::Throughput,
            1000,
            1050,
        );
        assert!(success.is_positive());

        let failure = FeedbackEntry::new(
            2,
            0x1234567890abcdef,
            1,
            FeedbackMetric::Throughput,
            1000,
            950,
        );
        assert!(!failure.is_positive());
    }

    #[test]
    fn test_learning_pattern_creation() {
        let pattern = LearningPattern::new(1, 0x1234567890abcdef, 1, FeedbackMetric::Throughput);
        assert_eq!(pattern.id, 1);
        assert_eq!(pattern.success_rate, 0);
        assert!(!pattern.is_mature());
    }

    #[test]
    fn test_learning_pattern_record_success() {
        let mut pattern = LearningPattern::new(1, 0x1234567890abcdef, 1, FeedbackMetric::Throughput);
        pattern.record_success(15);
        assert_eq!(pattern.success_count, 1);
        assert_eq!(pattern.application_count, 1);
        assert_eq!(pattern.success_rate, 100);
        assert_eq!(pattern.avg_improvement, 15);
    }

    #[test]
    fn test_learning_pattern_record_failure() {
        let mut pattern = LearningPattern::new(1, 0x1234567890abcdef, 1, FeedbackMetric::Throughput);
        pattern.record_success(15);
        pattern.record_failure();
        assert_eq!(pattern.application_count, 2);
        assert_eq!(pattern.success_count, 1);
        assert_eq!(pattern.success_rate, 50);
    }

    #[test]
    fn test_learning_pattern_maturity() {
        let mut pattern = LearningPattern::new(1, 0x1234567890abcdef, 1, FeedbackMetric::Throughput);
        for _ in 0..5 {
            pattern.record_success(10);
        }
        assert!(pattern.is_mature());
    }

    #[test]
    fn test_learning_pattern_reliability_score() {
        let mut pattern = LearningPattern::new(1, 0x1234567890abcdef, 1, FeedbackMetric::Throughput);
        for _ in 0..5 {
            pattern.record_success(50);
        }
        let score = pattern.reliability_score();
        assert!(score > 50);
    }

    #[test]
    fn test_adaptive_strategy_creation() {
        let strategy = AdaptiveStrategy::new(1, 0x1234567890abcdef);
        assert_eq!(strategy.id, 1);
        assert_eq!(strategy.confidence, 0);
        assert_eq!(strategy.applied_count, 0);
    }

    #[test]
    fn test_adaptive_strategy_update() {
        let mut strategy = AdaptiveStrategy::new(1, 0x1234567890abcdef);
        strategy.update(true);
        assert_eq!(strategy.applied_count, 1);
        assert_eq!(strategy.success_count, 1);
        assert_eq!(strategy.confidence, 100);
    }

    #[test]
    fn test_adaptive_strategy_set_preference() {
        let mut strategy = AdaptiveStrategy::new(1, 0x1234567890abcdef);
        strategy.set_preference(2, FeedbackMetric::MemoryUsage);
        assert_eq!(strategy.preferred_mutation, 2);
        assert_eq!(strategy.preferred_metric, FeedbackMetric::MemoryUsage);
    }

    #[test]
    fn test_adaptive_strategy_effectiveness() {
        let mut strategy = AdaptiveStrategy::new(1, 0x1234567890abcdef);
        strategy.update(true);
        strategy.update(true);
        strategy.update(false);
        let effectiveness = strategy.effectiveness();
        assert!(effectiveness >= 66 && effectiveness <= 67);  // 2/3
    }

    #[test]
    fn test_feedback_loop_creation() {
        let loop_ctrl = FeedbackLoop::new();
        assert_eq!(loop_ctrl.total_feedback(), 0);
        assert!(loop_ctrl.learning_enabled);
    }

    #[test]
    fn test_feedback_loop_submit_feedback() {
        let mut loop_ctrl = FeedbackLoop::new();
        let entry = FeedbackEntry::new(
            1,
            0x1234567890abcdef,
            1,
            FeedbackMetric::Throughput,
            1000,
            1100,
        );
        assert!(loop_ctrl.submit_feedback(entry));
        assert_eq!(loop_ctrl.total_feedback(), 1);
    }

    #[test]
    fn test_feedback_loop_learn_from_entry() {
        let mut loop_ctrl = FeedbackLoop::new();
        let entry = FeedbackEntry::new(
            1,
            0x1234567890abcdef,
            1,
            FeedbackMetric::Throughput,
            1000,
            1100,
        );
        assert!(loop_ctrl.learn_from_entry(entry));
    }

    #[test]
    fn test_feedback_loop_update_strategy() {
        let mut loop_ctrl = FeedbackLoop::new();
        assert!(loop_ctrl.update_strategy(0x1234567890abcdef, true));
    }

    #[test]
    fn test_feedback_loop_best_pattern_for_target() {
        let mut loop_ctrl = FeedbackLoop::new();

        let entry = FeedbackEntry::new(
            1,
            0x1234567890abcdef,
            1,
            FeedbackMetric::Throughput,
            1000,
            1100,
        );
        loop_ctrl.learn_from_entry(entry);

        // Need 5 applications to be mature
        for _ in 0..5 {
            loop_ctrl.learn_from_entry(entry);
        }

        let pattern = loop_ctrl.best_pattern_for_target(0x1234567890abcdef);
        assert!(pattern.is_some());
    }

    #[test]
    fn test_feedback_loop_get_strategy() {
        let mut loop_ctrl = FeedbackLoop::new();
        loop_ctrl.update_strategy(0x1234567890abcdef, true);

        let strategy = loop_ctrl.get_strategy(0x1234567890abcdef);
        assert!(strategy.is_some());
    }

    #[test]
    fn test_feedback_loop_success_rate() {
        let mut loop_ctrl = FeedbackLoop::new();

        let success = FeedbackEntry::new(
            1,
            0x1234567890abcdef,
            1,
            FeedbackMetric::Throughput,
            1000,
            1100,
        );
        let failure = FeedbackEntry::new(
            2,
            0x1234567890abcdef,
            1,
            FeedbackMetric::Throughput,
            1000,
            950,
        );

        loop_ctrl.submit_feedback(success);
        loop_ctrl.submit_feedback(failure);

        let rate = loop_ctrl.success_rate(0x1234567890abcdef, 1);
        assert_eq!(rate, 50);
    }

    #[test]
    fn test_feedback_loop_learning_statistics() {
        let mut loop_ctrl = FeedbackLoop::new();
        let entry = FeedbackEntry::new(
            1,
            0x1234567890abcdef,
            1,
            FeedbackMetric::Throughput,
            1000,
            1100,
        );

        for _ in 0..5 {
            loop_ctrl.learn_from_entry(entry);
        }

        let (total, mature, _avg) = loop_ctrl.learning_statistics();
        assert_eq!(total, 1);
        assert_eq!(mature, 1);
    }

    #[test]
    fn test_feedback_loop_learning_disabled() {
        let mut loop_ctrl = FeedbackLoop::new();
        loop_ctrl.set_learning_enabled(false);

        let entry = FeedbackEntry::new(
            1,
            0x1234567890abcdef,
            1,
            FeedbackMetric::Throughput,
            1000,
            1100,
        );
        assert!(!loop_ctrl.submit_feedback(entry));
    }

    #[test]
    fn test_feedback_loop_top_patterns() {
        let mut loop_ctrl = FeedbackLoop::new();

        for i in 0..3 {
            let entry = FeedbackEntry::new(
                i as u32,
                0x1000 + i as u64,
                i as u8,
                FeedbackMetric::Throughput,
                1000,
                1000 + (i as u32) * 50,
            );

            for _ in 0..5 {
                loop_ctrl.learn_from_entry(entry);
            }
        }

        let top = loop_ctrl.top_patterns(3);
        assert!(top[0].is_some());
    }

    #[test]
    fn test_feedback_loop_max_entries() {
        let mut loop_ctrl = FeedbackLoop::new();

        for i in 0..100 {
            let entry = FeedbackEntry::new(
                i,
                0x1234567890abcdef,
                1,
                FeedbackMetric::Throughput,
                1000,
                1100,
            );
            assert!(loop_ctrl.submit_feedback(entry));
        }

        // 101st should fail
        let entry = FeedbackEntry::new(
            100,
            0x1234567890abcdef,
            1,
            FeedbackMetric::Throughput,
            1000,
            1100,
        );
        assert!(!loop_ctrl.submit_feedback(entry));
    }

    #[test]
    fn test_feedback_entry_outcome_classification() {
        // Great success: > 10%
        let great = FeedbackEntry::new(1, 0x1111, 1, FeedbackMetric::Throughput, 100, 112);
        assert_eq!(great.outcome, OutcomeType::GreatSuccess);

        // Success: 3-10%
        let good = FeedbackEntry::new(2, 0x2222, 1, FeedbackMetric::Throughput, 100, 105);
        assert_eq!(good.outcome, OutcomeType::Success);

        // Neutral: -2 to 3
        let neutral = FeedbackEntry::new(3, 0x3333, 1, FeedbackMetric::Throughput, 100, 101);
        assert_eq!(neutral.outcome, OutcomeType::Neutral);

        // Minor failure: -2 to -10
        let minor = FeedbackEntry::new(4, 0x4444, 1, FeedbackMetric::Throughput, 100, 95);
        assert_eq!(minor.outcome, OutcomeType::MinorFailure);

        // Critical: < -10
        let critical = FeedbackEntry::new(5, 0x5555, 1, FeedbackMetric::Throughput, 100, 85);
        assert_eq!(critical.outcome, OutcomeType::CriticalFailure);
    }

    #[test]
    fn test_learning_pattern_moving_average() {
        let mut pattern = LearningPattern::new(1, 0x1234567890abcdef, 1, FeedbackMetric::Throughput);
        pattern.record_success(10);
        pattern.record_success(20);
        pattern.record_success(30);

        // Average should be around 20
        assert!(pattern.avg_improvement >= 19 && pattern.avg_improvement <= 21);
    }
}
