//! Multi-objective Optimizer: Pareto-Optimal Mutation Selection
//!
//! Balances competing objectives (performance, power, security) without dominance.
//! Uses Pareto frontier analysis to identify non-dominated mutation candidates
//! and trades off between conflicting metrics.
//!
//! Phase 34, Task 5

/// Optimization objective type
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ObjectiveType {
    Performance = 0,    // Maximize throughput/minimize latency
    PowerEfficiency = 1, // Minimize energy consumption
    Security = 2,       // Maximize security score (hardening)
    Memory = 3,         // Minimize memory usage
    Reliability = 4,    // Maximize stability
}

/// Objective weight for weighted scoring (0-100)
#[derive(Clone, Copy, Debug)]
pub struct ObjectiveWeight {
    pub performance: u8,
    pub power_efficiency: u8,
    pub security: u8,
    pub memory: u8,
    pub reliability: u8,
}

impl ObjectiveWeight {
    /// Create weight profile with equal distribution
    pub const fn balanced() -> Self {
        ObjectiveWeight {
            performance: 20,
            power_efficiency: 20,
            security: 20,
            memory: 20,
            reliability: 20,
        }
    }

    /// Performance-focused weights (40% perf, others 15%)
    pub const fn performance_focused() -> Self {
        ObjectiveWeight {
            performance: 40,
            power_efficiency: 15,
            security: 15,
            memory: 15,
            reliability: 15,
        }
    }

    /// Power-focused weights
    pub const fn power_focused() -> Self {
        ObjectiveWeight {
            performance: 15,
            power_efficiency: 40,
            security: 15,
            memory: 15,
            reliability: 15,
        }
    }

    /// Security-focused weights
    pub const fn security_focused() -> Self {
        ObjectiveWeight {
            performance: 15,
            power_efficiency: 15,
            security: 40,
            memory: 15,
            reliability: 15,
        }
    }

    /// Get weight for objective
    pub fn weight_for(&self, objective: ObjectiveType) -> u8 {
        match objective {
            ObjectiveType::Performance => self.performance,
            ObjectiveType::PowerEfficiency => self.power_efficiency,
            ObjectiveType::Security => self.security,
            ObjectiveType::Memory => self.memory,
            ObjectiveType::Reliability => self.reliability,
        }
    }

    /// Normalize weights to sum to 100
    pub fn normalize(&self) -> ObjectiveWeight {
        let total = self.performance as u32 + self.power_efficiency as u32
            + self.security as u32 + self.memory as u32 + self.reliability as u32;

        if total == 0 {
            return ObjectiveWeight::balanced();
        }

        ObjectiveWeight {
            performance: ((self.performance as u32 * 100) / total) as u8,
            power_efficiency: ((self.power_efficiency as u32 * 100) / total) as u8,
            security: ((self.security as u32 * 100) / total) as u8,
            memory: ((self.memory as u32 * 100) / total) as u8,
            reliability: ((self.reliability as u32 * 100) / total) as u8,
        }
    }
}

/// Multi-objective fitness score
#[derive(Clone, Copy, Debug)]
pub struct MultiObjectiveScore {
    /// Performance score (0-100, higher better)
    pub performance: u8,
    /// Power efficiency score (0-100, higher better)
    pub power_efficiency: u8,
    /// Security score (0-100, higher better)
    pub security: u8,
    /// Memory score (0-100, higher better - less memory is better)
    pub memory: u8,
    /// Reliability score (0-100, higher better)
    pub reliability: u8,
}

impl MultiObjectiveScore {
    /// Create new multi-objective score
    pub const fn new(perf: u8, power: u8, sec: u8, mem: u8, rel: u8) -> Self {
        MultiObjectiveScore {
            performance: if perf > 100 { 100 } else { perf },
            power_efficiency: if power > 100 { 100 } else { power },
            security: if sec > 100 { 100 } else { sec },
            memory: if mem > 100 { 100 } else { mem },
            reliability: if rel > 100 { 100 } else { rel },
        }
    }

    /// Calculate weighted score
    pub fn weighted_score(&self, weights: &ObjectiveWeight) -> u16 {
        let perf = self.performance as u16 * weights.performance as u16;
        let power = self.power_efficiency as u16 * weights.power_efficiency as u16;
        let security = self.security as u16 * weights.security as u16;
        let memory = self.memory as u16 * weights.memory as u16;
        let reliability = self.reliability as u16 * weights.reliability as u16;

        (perf + power + security + memory + reliability) / 100
    }

    /// Get average score across objectives
    pub fn average_score(&self) -> u8 {
        ((self.performance as u16 + self.power_efficiency as u16 + self.security as u16
            + self.memory as u16 + self.reliability as u16) / 5) as u8
    }

    /// Dominates another score (strictly better or equal in all, strictly better in at least one)
    pub fn dominates(&self, other: &Self) -> bool {
        let self_better_or_equal = self.performance >= other.performance
            && self.power_efficiency >= other.power_efficiency
            && self.security >= other.security
            && self.memory >= other.memory
            && self.reliability >= other.reliability;

        let strictly_better = self.performance > other.performance
            || self.power_efficiency > other.power_efficiency
            || self.security > other.security
            || self.memory > other.memory
            || self.reliability > other.reliability;

        self_better_or_equal && strictly_better
    }

    /// Pareto distance to another score (sum of squared differences)
    pub fn distance_to(&self, other: &Self) -> u32 {
        let perf_diff = (self.performance as i16 - other.performance as i16).abs() as u32;
        let power_diff = (self.power_efficiency as i16 - other.power_efficiency as i16).abs() as u32;
        let sec_diff = (self.security as i16 - other.security as i16).abs() as u32;
        let mem_diff = (self.memory as i16 - other.memory as i16).abs() as u32;
        let rel_diff = (self.reliability as i16 - other.reliability as i16).abs() as u32;

        perf_diff * perf_diff + power_diff * power_diff + sec_diff * sec_diff
            + mem_diff * mem_diff + rel_diff * rel_diff
    }
}

/// Pareto frontier point (mutation + its score)
#[derive(Clone, Copy, Debug)]
pub struct ParetoPoint {
    /// Mutation ID
    pub mutation_id: u32,
    /// Target function hash
    pub target_hash: u64,
    /// Multi-objective score
    pub score: MultiObjectiveScore,
}

impl ParetoPoint {
    /// Create new Pareto point
    pub const fn new(mutation_id: u32, target_hash: u64, score: MultiObjectiveScore) -> Self {
        ParetoPoint {
            mutation_id,
            target_hash,
            score,
        }
    }

    /// Is dominated by another point
    pub fn is_dominated_by(&self, other: &Self) -> bool {
        other.score.dominates(&self.score)
    }
}

/// Trade-off analysis result
#[derive(Clone, Copy, Debug)]
pub struct TradeOffAnalysis {
    /// Maximum performance achievable
    pub max_performance: u8,
    /// Maximum power efficiency achievable
    pub max_power_efficiency: u8,
    /// Maximum security achievable
    pub max_security: u8,
    /// Minimum memory usage achievable
    pub min_memory: u8,
    /// Maximum reliability achievable
    pub max_reliability: u8,
    /// Trade-off ratio (performance vs power)
    pub perf_power_ratio: u16,
    /// Trade-off ratio (performance vs security)
    pub perf_security_ratio: u16,
}

/// Mutation candidate with multi-objective evaluation
#[derive(Clone, Copy, Debug)]
pub struct MultiObjectiveCandidate {
    /// Candidate ID
    pub id: u32,
    /// Target function hash
    pub target_hash: u64,
    /// Mutation type applied
    pub mutation_type: u8,
    /// Multi-objective fitness
    pub score: MultiObjectiveScore,
    /// Pareto rank (lower is better)
    pub pareto_rank: u8,
    /// On Pareto frontier
    pub on_frontier: bool,
}

/// Multi-objective Optimizer
pub struct MultiObjectiveOptimizer {
    /// Pareto frontier points (max 100)
    frontier: [Option<ParetoPoint>; 100],
    /// All evaluated candidates (max 200)
    candidates: [Option<MultiObjectiveCandidate>; 200],
    /// Current weights
    weights: ObjectiveWeight,
    /// Total candidates evaluated
    total_candidates: u32,
    /// Total frontier updates
    frontier_updates: u32,
}

impl MultiObjectiveOptimizer {
    /// Create new multi-objective optimizer
    pub const fn new() -> Self {
        MultiObjectiveOptimizer {
            frontier: [None; 100],
            candidates: [None; 200],
            weights: ObjectiveWeight::balanced(),
            total_candidates: 0,
            frontier_updates: 0,
        }
    }

    /// Set objective weights
    pub fn set_weights(&mut self, weights: ObjectiveWeight) {
        self.weights = weights.normalize();
    }

    /// Evaluate and add candidate to Pareto frontier
    pub fn evaluate_candidate(&mut self, id: u32, target_hash: u64, score: MultiObjectiveScore) -> bool {
        // Store candidate
        for slot in &mut self.candidates {
            if slot.is_none() {
                // Calculate Pareto rank
                let mut rank = 0;
                for frontier_slot in &self.frontier {
                    if let Some(point) = frontier_slot {
                        if point.score.dominates(&score) {
                            rank += 1;
                        }
                    }
                }

                let on_frontier = rank == 0;

                *slot = Some(MultiObjectiveCandidate {
                    id,
                    target_hash,
                    mutation_type: 0,
                    score,
                    pareto_rank: rank.min(255) as u8,
                    on_frontier,
                });

                self.total_candidates += 1;

                // Update frontier
                if on_frontier {
                    self.add_to_frontier(ParetoPoint::new(id, target_hash, score));
                }

                return true;
            }
        }
        false
    }

    /// Add point to Pareto frontier, removing dominated points
    fn add_to_frontier(&mut self, new_point: ParetoPoint) {
        // Remove points dominated by new point
        for slot in &mut self.frontier {
            if let Some(point) = slot {
                if new_point.score.dominates(&point.score) {
                    *slot = None;
                }
            }
        }

        // Add new point if not dominated
        let mut dominated = false;
        for slot in &self.frontier {
            if let Some(point) = slot {
                if point.score.dominates(&new_point.score) {
                    dominated = true;
                    break;
                }
            }
        }

        if !dominated {
            for slot in &mut self.frontier {
                if slot.is_none() {
                    *slot = Some(new_point);
                    self.frontier_updates += 1;
                    return;
                }
            }
        }
    }

    /// Get Pareto frontier
    pub fn get_frontier(&self) -> [Option<ParetoPoint>; 100] {
        self.frontier
    }

    /// Get frontier size
    pub fn frontier_size(&self) -> usize {
        self.frontier.iter().filter(|slot| slot.is_some()).count()
    }

    /// Get best candidate by weighted score
    pub fn best_by_weight(&self) -> Option<MultiObjectiveCandidate> {
        let mut best = None;
        let mut best_score = 0;

        for slot in &self.candidates {
            if let Some(candidate) = slot {
                let score = candidate.score.weighted_score(&self.weights);
                if score > best_score {
                    best_score = score;
                    best = Some(*candidate);
                }
            }
        }

        best
    }

    /// Get best candidate by average score
    pub fn best_by_average(&self) -> Option<MultiObjectiveCandidate> {
        let mut best = None;
        let mut best_score = 0;

        for slot in &self.candidates {
            if let Some(candidate) = slot {
                let score = candidate.score.average_score();
                if score > best_score {
                    best_score = score;
                    best = Some(*candidate);
                }
            }
        }

        best
    }

    /// Analyze trade-offs on Pareto frontier
    pub fn analyze_trade_offs(&self) -> Option<TradeOffAnalysis> {
        let frontier_size = self.frontier_size();
        if frontier_size == 0 {
            return None;
        }

        let mut max_perf = 0;
        let mut max_power = 0;
        let mut max_sec = 0;
        let mut min_mem = 255;
        let mut max_rel = 0;

        for slot in &self.frontier {
            if let Some(point) = slot {
                max_perf = max_perf.max(point.score.performance);
                max_power = max_power.max(point.score.power_efficiency);
                max_sec = max_sec.max(point.score.security);
                min_mem = min_mem.min(point.score.memory);
                max_rel = max_rel.max(point.score.reliability);
            }
        }

        let perf_power_ratio = if max_power > 0 {
            (max_perf as u16 * 100) / max_power as u16
        } else {
            0
        };

        let perf_security_ratio = if max_sec > 0 {
            (max_perf as u16 * 100) / max_sec as u16
        } else {
            0
        };

        Some(TradeOffAnalysis {
            max_performance: max_perf,
            max_power_efficiency: max_power,
            max_security: max_sec,
            min_memory: min_mem,
            max_reliability: max_rel,
            perf_power_ratio,
            perf_security_ratio,
        })
    }

    /// Get all frontier points as candidates
    pub fn frontier_as_candidates(&self) -> [Option<MultiObjectiveCandidate>; 100] {
        let mut result = [None; 100];

        for (i, slot) in self.frontier.iter().enumerate() {
            if let Some(point) = slot {
                result[i] = Some(MultiObjectiveCandidate {
                    id: point.mutation_id,
                    target_hash: point.target_hash,
                    mutation_type: 0,
                    score: point.score,
                    pareto_rank: 0,
                    on_frontier: true,
                });
            }
        }

        result
    }

    /// Get candidates in objective region (min/max bounds)
    pub fn candidates_in_region(
        &self,
        perf_min: u8,
        power_min: u8,
        sec_min: u8,
    ) -> [Option<MultiObjectiveCandidate>; 100] {
        let mut result = [None; 100];
        let mut idx = 0;

        for slot in &self.candidates {
            if let Some(candidate) = slot {
                if candidate.score.performance >= perf_min
                    && candidate.score.power_efficiency >= power_min
                    && candidate.score.security >= sec_min
                {
                    if idx < 100 {
                        result[idx] = Some(*candidate);
                        idx += 1;
                    }
                }
            }
        }

        result
    }

    /// Get most balanced candidate (closest to ideal point)
    pub fn most_balanced(&self) -> Option<MultiObjectiveCandidate> {
        let ideal = MultiObjectiveScore::new(100, 100, 100, 100, 100);
        let mut best = None;
        let mut best_distance = u32::MAX;

        for slot in &self.candidates {
            if let Some(candidate) = slot {
                let distance = ideal.distance_to(&candidate.score);
                if distance < best_distance {
                    best_distance = distance;
                    best = Some(*candidate);
                }
            }
        }

        best
    }

    /// Statistics about optimizer state
    pub fn statistics(&self) -> (u32, u32, u32) {
        let frontier_count = self.frontier_size() as u32;
        let candidate_count = self.candidates.iter().filter(|s| s.is_some()).count() as u32;
        (frontier_count, candidate_count, self.total_candidates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_objective_type_enum() {
        assert_eq!(ObjectiveType::Performance as u8, 0);
        assert_eq!(ObjectiveType::Security as u8, 2);
        assert_eq!(ObjectiveType::Reliability as u8, 4);
    }

    #[test]
    fn test_objective_weight_balanced() {
        let weights = ObjectiveWeight::balanced();
        assert_eq!(weights.performance, 20);
        assert_eq!(weights.power_efficiency, 20);
    }

    #[test]
    fn test_objective_weight_performance_focused() {
        let weights = ObjectiveWeight::performance_focused();
        assert_eq!(weights.performance, 40);
        assert!(weights.power_efficiency <= 20);
    }

    #[test]
    fn test_objective_weight_normalize() {
        let weights = ObjectiveWeight {
            performance: 50,
            power_efficiency: 50,
            security: 0,
            memory: 0,
            reliability: 0,
        };
        let normalized = weights.normalize();
        assert_eq!(normalized.performance, 50);
        assert_eq!(normalized.power_efficiency, 50);
    }

    #[test]
    fn test_multi_objective_score_creation() {
        let score = MultiObjectiveScore::new(80, 75, 90, 70, 85);
        assert_eq!(score.performance, 80);
        assert_eq!(score.security, 90);
    }

    #[test]
    fn test_multi_objective_score_weighted_score() {
        let score = MultiObjectiveScore::new(100, 100, 100, 100, 100);
        let weights = ObjectiveWeight::balanced();
        let weighted = score.weighted_score(&weights);
        assert_eq!(weighted, 100);
    }

    #[test]
    fn test_multi_objective_score_average() {
        let score = MultiObjectiveScore::new(50, 60, 70, 80, 90);
        let avg = score.average_score();
        assert!(avg >= 69 && avg <= 71);
    }

    #[test]
    fn test_multi_objective_score_dominates() {
        let score1 = MultiObjectiveScore::new(100, 100, 100, 100, 100);
        let score2 = MultiObjectiveScore::new(50, 50, 50, 50, 50);
        assert!(score1.dominates(&score2));
        assert!(!score2.dominates(&score1));
    }

    #[test]
    fn test_multi_objective_score_dominates_equal() {
        let score1 = MultiObjectiveScore::new(80, 80, 80, 80, 80);
        let score2 = MultiObjectiveScore::new(80, 80, 80, 80, 80);
        assert!(!score1.dominates(&score2));
    }

    #[test]
    fn test_multi_objective_score_distance() {
        let score1 = MultiObjectiveScore::new(100, 100, 100, 100, 100);
        let score2 = MultiObjectiveScore::new(50, 50, 50, 50, 50);
        let distance = score1.distance_to(&score2);
        assert!(distance > 0);
    }

    #[test]
    fn test_pareto_point_creation() {
        let score = MultiObjectiveScore::new(80, 75, 90, 70, 85);
        let point = ParetoPoint::new(1, 0x1234567890abcdef, score);
        assert_eq!(point.mutation_id, 1);
        assert!(!point.is_dominated_by(&point));
    }

    #[test]
    fn test_trade_off_analysis_creation() {
        let analysis = TradeOffAnalysis {
            max_performance: 95,
            max_power_efficiency: 85,
            max_security: 90,
            min_memory: 60,
            max_reliability: 92,
            perf_power_ratio: 111,
            perf_security_ratio: 105,
        };
        assert_eq!(analysis.max_performance, 95);
    }

    #[test]
    fn test_multi_objective_candidate_creation() {
        let score = MultiObjectiveScore::new(80, 75, 90, 70, 85);
        let candidate = MultiObjectiveCandidate {
            id: 1,
            target_hash: 0x1234567890abcdef,
            mutation_type: 1,
            score,
            pareto_rank: 0,
            on_frontier: true,
        };
        assert!(candidate.on_frontier);
    }

    #[test]
    fn test_optimizer_creation() {
        let optimizer = MultiObjectiveOptimizer::new();
        assert_eq!(optimizer.frontier_size(), 0);
        assert_eq!(optimizer.total_candidates, 0);
    }

    #[test]
    fn test_optimizer_set_weights() {
        let mut optimizer = MultiObjectiveOptimizer::new();
        optimizer.set_weights(ObjectiveWeight::security_focused());
        assert_eq!(optimizer.weights.security, 40);
    }

    #[test]
    fn test_optimizer_evaluate_single_candidate() {
        let mut optimizer = MultiObjectiveOptimizer::new();
        let score = MultiObjectiveScore::new(80, 75, 90, 70, 85);
        assert!(optimizer.evaluate_candidate(1, 0x1111111111111111, score));
        assert_eq!(optimizer.frontier_size(), 1);
    }

    #[test]
    fn test_optimizer_pareto_frontier() {
        let mut optimizer = MultiObjectiveOptimizer::new();

        let score1 = MultiObjectiveScore::new(90, 70, 80, 80, 85);
        let score2 = MultiObjectiveScore::new(70, 90, 80, 80, 85);
        let score3 = MultiObjectiveScore::new(50, 50, 50, 50, 50);

        assert!(optimizer.evaluate_candidate(1, 0x1111, score1));
        assert!(optimizer.evaluate_candidate(2, 0x2222, score2));
        assert!(optimizer.evaluate_candidate(3, 0x3333, score3));

        // score3 is dominated, frontier size should be 2
        assert_eq!(optimizer.frontier_size(), 2);
    }

    #[test]
    fn test_optimizer_best_by_weight() {
        let mut optimizer = MultiObjectiveOptimizer::new();
        optimizer.set_weights(ObjectiveWeight::performance_focused());

        let score1 = MultiObjectiveScore::new(95, 60, 60, 60, 60);
        let score2 = MultiObjectiveScore::new(70, 95, 70, 70, 70);

        optimizer.evaluate_candidate(1, 0x1111, score1);
        optimizer.evaluate_candidate(2, 0x2222, score2);

        let best = optimizer.best_by_weight();
        assert!(best.is_some());
        assert_eq!(best.unwrap().id, 1);  // Performance-focused should pick score1
    }

    #[test]
    fn test_optimizer_best_by_average() {
        let mut optimizer = MultiObjectiveOptimizer::new();

        let score1 = MultiObjectiveScore::new(100, 100, 100, 100, 100);
        let score2 = MultiObjectiveScore::new(50, 50, 50, 50, 50);

        optimizer.evaluate_candidate(1, 0x1111, score1);
        optimizer.evaluate_candidate(2, 0x2222, score2);

        let best = optimizer.best_by_average();
        assert!(best.is_some());
        assert_eq!(best.unwrap().id, 1);
    }

    #[test]
    fn test_optimizer_analyze_trade_offs() {
        let mut optimizer = MultiObjectiveOptimizer::new();

        let score1 = MultiObjectiveScore::new(95, 70, 80, 85, 90);
        let score2 = MultiObjectiveScore::new(70, 95, 80, 85, 90);

        optimizer.evaluate_candidate(1, 0x1111, score1);
        optimizer.evaluate_candidate(2, 0x2222, score2);

        let analysis = optimizer.analyze_trade_offs();
        assert!(analysis.is_some());
        let analysis = analysis.unwrap();
        assert_eq!(analysis.max_performance, 95);
        assert_eq!(analysis.max_power_efficiency, 95);
    }

    #[test]
    fn test_optimizer_frontier_as_candidates() {
        let mut optimizer = MultiObjectiveOptimizer::new();

        let score1 = MultiObjectiveScore::new(90, 70, 80, 80, 85);
        let score2 = MultiObjectiveScore::new(70, 90, 80, 80, 85);

        optimizer.evaluate_candidate(1, 0x1111, score1);
        optimizer.evaluate_candidate(2, 0x2222, score2);

        let frontier_candidates = optimizer.frontier_as_candidates();
        let count = frontier_candidates.iter().filter(|s| s.is_some()).count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_optimizer_candidates_in_region() {
        let mut optimizer = MultiObjectiveOptimizer::new();

        let score1 = MultiObjectiveScore::new(95, 85, 80, 80, 90);
        let score2 = MultiObjectiveScore::new(50, 50, 50, 50, 50);

        optimizer.evaluate_candidate(1, 0x1111, score1);
        optimizer.evaluate_candidate(2, 0x2222, score2);

        let region = optimizer.candidates_in_region(90, 80, 75);
        let count = region.iter().filter(|s| s.is_some()).count();
        assert_eq!(count, 1);  // Only score1 meets the region criteria
    }

    #[test]
    fn test_optimizer_most_balanced() {
        let mut optimizer = MultiObjectiveOptimizer::new();

        let score1 = MultiObjectiveScore::new(100, 100, 100, 100, 100);
        let score2 = MultiObjectiveScore::new(50, 50, 50, 50, 50);

        optimizer.evaluate_candidate(1, 0x1111, score1);
        optimizer.evaluate_candidate(2, 0x2222, score2);

        let balanced = optimizer.most_balanced();
        assert!(balanced.is_some());
        assert_eq!(balanced.unwrap().id, 1);  // Perfectly balanced candidate
    }

    #[test]
    fn test_optimizer_statistics() {
        let mut optimizer = MultiObjectiveOptimizer::new();

        let score = MultiObjectiveScore::new(80, 75, 90, 70, 85);
        optimizer.evaluate_candidate(1, 0x1111, score);
        optimizer.evaluate_candidate(2, 0x2222, score);
        optimizer.evaluate_candidate(3, 0x3333, score);

        let (frontier, candidates, total) = optimizer.statistics();
        assert!(frontier > 0);
        assert!(candidates > 0);
        assert_eq!(total, 3);
    }

    #[test]
    fn test_objective_weight_for() {
        let weights = ObjectiveWeight::performance_focused();
        assert_eq!(weights.weight_for(ObjectiveType::Performance), 40);
        assert_eq!(weights.weight_for(ObjectiveType::Security), 15);
    }

    #[test]
    fn test_pareto_dominance_partial() {
        let score1 = MultiObjectiveScore::new(90, 70, 90, 80, 85);
        let score2 = MultiObjectiveScore::new(85, 85, 85, 85, 85);

        // score1 and score2 have mixed dominance - neither dominates
        assert!(!score1.dominates(&score2));
        assert!(!score2.dominates(&score1));
    }

    #[test]
    fn test_optimizer_max_candidates() {
        let mut optimizer = MultiObjectiveOptimizer::new();

        let score = MultiObjectiveScore::new(80, 75, 90, 70, 85);

        // Try to add 200 candidates (max capacity)
        for i in 0..200 {
            let result = optimizer.evaluate_candidate(i, 0x1000 + i as u64, score);
            if i < 200 {
                assert!(result);
            }
        }

        // 201st should fail
        let result = optimizer.evaluate_candidate(200, 0x2000, score);
        assert!(!result);
    }

    #[test]
    fn test_weighted_score_with_different_weights() {
        let score = MultiObjectiveScore::new(80, 60, 90, 70, 75);

        let perf_focused = ObjectiveWeight::performance_focused();
        let power_focused = ObjectiveWeight::power_focused();

        let perf_score = score.weighted_score(&perf_focused);
        let power_score = score.weighted_score(&power_focused);

        // Performance-focused should score higher for this candidate
        assert!(perf_score > power_score);
    }
}
