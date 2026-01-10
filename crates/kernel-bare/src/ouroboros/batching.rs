//! Multi-Mutation Batching for Parallel Evolution
//!
//! Enables testing multiple mutations in parallel with adaptive batch sizing.
//! Manages resource allocation, dependency tracking, and result aggregation.
//!
//! Phase 32, Task 6

use core::mem;

/// Mutation batch identifier
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct BatchId(u32);

impl BatchId {
    /// Create new batch ID
    pub const fn new(id: u32) -> Self {
        BatchId(id)
    }

    /// Get numeric value
    pub const fn value(&self) -> u32 {
        self.0
    }
}

/// Mutation in a batch
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatchedMutation {
    /// Mutation ID
    pub mutation_id: u32,
    /// Batch this belongs to
    pub batch_id: BatchId,
    /// Resource estimate (CPU cycles)
    pub estimated_cycles: u32,
    /// Memory estimate (KB)
    pub estimated_memory: u32,
    /// Dependencies on other mutations (bitmask)
    pub dependencies: u16,
}

impl BatchedMutation {
    /// Create new batched mutation
    pub const fn new(
        mutation_id: u32,
        batch_id: BatchId,
        cycles: u32,
        memory: u32,
    ) -> Self {
        BatchedMutation {
            mutation_id,
            batch_id,
            estimated_cycles: cycles,
            estimated_memory: memory,
            dependencies: 0,
        }
    }

    /// Add dependency on another mutation
    pub fn add_dependency(&mut self, mutation_index: usize) {
        if mutation_index < 16 {
            self.dependencies |= 1u16 << mutation_index;
        }
    }

    /// Check if depends on mutation at index
    pub fn depends_on(&self, mutation_index: usize) -> bool {
        if mutation_index >= 16 {
            return false;
        }
        (self.dependencies & (1u16 << mutation_index)) != 0
    }
}

/// Evolution batch testing status
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum EvolutionBatchStatus {
    Queued = 0,
    Preparing = 1,
    Executing = 2,
    Complete = 3,
    Failed = 4,
}

/// Mutation result in batch
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MutationResult {
    /// Mutation ID
    pub mutation_id: u32,
    /// Passed tests
    pub passed: bool,
    /// Fitness improvement percent (scaled by 100)
    pub improvement: u32,
    /// Execution time (ms)
    pub duration_ms: u32,
    /// Actual memory used (KB)
    pub actual_memory: u32,
}

impl MutationResult {
    /// Create new result
    pub const fn new(mutation_id: u32, passed: bool) -> Self {
        MutationResult {
            mutation_id,
            passed,
            improvement: 0,
            duration_ms: 0,
            actual_memory: 0,
        }
    }
}

/// Evolution batch for parallel testing
pub struct EvolutionBatch {
    /// Batch ID
    pub id: BatchId,
    /// Mutations in this batch
    mutations: [Option<BatchedMutation>; 32],
    /// Results for mutations
    results: [Option<MutationResult>; 32],
    /// Mutation count
    mutation_count: usize,
    /// Batch status
    pub status: EvolutionBatchStatus,
    /// Total estimated cycles
    total_cycles: u32,
    /// Total estimated memory (KB)
    total_memory: u32,
}

impl EvolutionBatch {
    /// Create new batch
    pub const fn new(batch_id: BatchId) -> Self {
        EvolutionBatch {
            id: batch_id,
            mutations: [None; 32],
            results: [None; 32],
            mutation_count: 0,
            status: EvolutionBatchStatus::Queued,
            total_cycles: 0,
            total_memory: 0,
        }
    }

    /// Add mutation to batch
    pub fn add_mutation(&mut self, mutation: BatchedMutation) -> bool {
        if self.mutation_count >= 32 {
            return false;
        }
        self.mutations[self.mutation_count] = Some(mutation);
        self.total_cycles = self.total_cycles.saturating_add(mutation.estimated_cycles);
        self.total_memory = self.total_memory.saturating_add(mutation.estimated_memory);
        self.mutation_count += 1;
        true
    }

    /// Record result for mutation
    pub fn record_result(&mut self, result: MutationResult) -> bool {
        // Find mutation with this ID
        for i in 0..self.mutation_count {
            if let Some(mut_) = &self.mutations[i] {
                if mut_.mutation_id == result.mutation_id {
                    self.results[i] = Some(result);
                    return true;
                }
            }
        }
        false
    }

    /// Get mutation count
    pub fn count(&self) -> usize {
        self.mutation_count
    }

    /// Get result for mutation
    pub fn get_result(&self, mutation_id: u32) -> Option<MutationResult> {
        for i in 0..self.mutation_count {
            if let Some(result) = &self.results[i] {
                if result.mutation_id == mutation_id {
                    return Some(*result);
                }
            }
        }
        None
    }

    /// Get pass rate percent
    pub fn pass_rate(&self) -> u32 {
        if self.mutation_count == 0 {
            return 0;
        }
        let mut passed = 0usize;
        for result in &self.results {
            if let Some(res) = result {
                if res.passed {
                    passed += 1;
                }
            }
        }
        ((passed as u64 * 100) / self.mutation_count as u64) as u32
    }

    /// Get average improvement
    pub fn avg_improvement(&self) -> u32 {
        if self.mutation_count == 0 {
            return 0;
        }
        let mut sum = 0u64;
        let mut count = 0u32;
        for result in &self.results {
            if let Some(res) = result {
                if res.passed {
                    sum += res.improvement as u64;
                    count += 1;
                }
            }
        }
        if count == 0 {
            return 0;
        }
        (sum / count as u64) as u32
    }

    /// Check if batch ready to execute
    pub fn ready_to_execute(&self) -> bool {
        self.mutation_count > 0 && self.status == EvolutionBatchStatus::Queued
    }

    /// Mark batch as executing
    pub fn start_execution(&mut self) {
        if self.status == EvolutionBatchStatus::Queued || self.status == EvolutionBatchStatus::Preparing {
            self.status = EvolutionBatchStatus::Executing;
        }
    }

    /// Mark batch as complete
    pub fn complete(&mut self, success: bool) {
        self.status = if success {
            EvolutionBatchStatus::Complete
        } else {
            EvolutionBatchStatus::Failed
        };
    }
}

/// Parallel test runner
pub struct ParallelTestRunner {
    /// Maximum concurrent mutations
    max_concurrent: u32,
    /// Current concurrent mutations
    current_concurrent: u32,
    /// Total mutations executed
    total_executed: u32,
    /// Total mutations passed
    total_passed: u32,
}

impl ParallelTestRunner {
    /// Create new runner
    pub const fn new(max_concurrent: u32) -> Self {
        let effective_max = if max_concurrent > 0 { max_concurrent } else { 1 };
        ParallelTestRunner {
            max_concurrent: effective_max,
            current_concurrent: 0,
            total_executed: 0,
            total_passed: 0,
        }
    }

    /// Check if can run more mutations
    pub fn can_run_more(&self) -> bool {
        self.current_concurrent < self.max_concurrent
    }

    /// Start mutation execution
    pub fn start_mutation(&mut self) -> bool {
        if self.can_run_more() {
            self.current_concurrent += 1;
            self.total_executed += 1;
            return true;
        }
        false
    }

    /// Complete mutation execution
    pub fn finish_mutation(&mut self, passed: bool) {
        if self.current_concurrent > 0 {
            self.current_concurrent -= 1;
            if passed {
                self.total_passed += 1;
            }
        }
    }

    /// Get overall pass rate
    pub fn pass_rate(&self) -> u32 {
        if self.total_executed == 0 {
            return 0;
        }
        ((self.total_passed as u64 * 100) / self.total_executed as u64) as u32
    }

    /// Get current concurrency utilization percent
    pub fn utilization(&self) -> u32 {
        ((self.current_concurrent as u64 * 100) / self.max_concurrent as u64) as u32
    }

    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.total_executed = 0;
        self.total_passed = 0;
    }
}

/// Adaptive batch sizer
pub struct AdaptiveBatcher {
    /// Minimum batch size
    min_size: u32,
    /// Maximum batch size
    max_size: u32,
    /// Current batch size
    current_size: u32,
    /// Recent success rate (scaled by 100)
    recent_success_rate: u32,
    /// Recent performance improvement (scaled by 100)
    recent_improvement: u32,
}

impl AdaptiveBatcher {
    /// Create new adaptive batcher
    pub const fn new(min_size: u32, max_size: u32) -> Self {
        let effective_min = if min_size > 0 { min_size } else { 1 };
        let effective_max = if max_size >= effective_min { max_size } else { effective_min };
        AdaptiveBatcher {
            min_size: effective_min,
            max_size: effective_max,
            current_size: min_size,
            recent_success_rate: 5000, // 50%
            recent_improvement: 0,
        }
    }

    /// Update statistics
    pub fn update_stats(&mut self, success_rate: u32, improvement: u32) {
        // Exponential moving average
        self.recent_success_rate =
            ((self.recent_success_rate as u64 * 70 + success_rate as u64 * 30) / 100) as u32;
        self.recent_improvement =
            ((self.recent_improvement as u64 * 70 + improvement as u64 * 30) / 100) as u32;

        self.adapt_batch_size();
    }

    /// Adapt batch size based on performance
    fn adapt_batch_size(&mut self) {
        // If success rate high and improvement good, increase batch size
        if self.recent_success_rate > 7000 && self.recent_improvement > 500 {
            self.current_size = (self.current_size + 1).min(self.max_size);
        }
        // If success rate low, decrease batch size
        else if self.recent_success_rate < 3000 {
            self.current_size = (self.current_size.saturating_sub(1)).max(self.min_size);
        }
        // Otherwise keep current size
    }

    /// Get recommended batch size for next batch
    pub fn recommend_size(&self) -> u32 {
        self.current_size
    }

    /// Get size range
    pub fn size_range(&self) -> (u32, u32) {
        (self.min_size, self.max_size)
    }

    /// Get current success rate
    pub fn success_rate(&self) -> u32 {
        self.recent_success_rate
    }

    /// Get current improvement rate
    pub fn improvement_rate(&self) -> u32 {
        self.recent_improvement
    }
}

/// Batch statistics
#[derive(Clone, Copy, Debug)]
pub struct BatchStatistics {
    /// Total batches executed
    pub total_batches: u32,
    /// Successful batches
    pub successful_batches: u32,
    /// Total mutations tested
    pub total_mutations: u32,
    /// Passed mutations
    pub passed_mutations: u32,
    /// Average batch size
    pub avg_batch_size: u32,
    /// Total time (ms)
    pub total_time_ms: u32,
}

impl BatchStatistics {
    /// Create new batch statistics
    pub const fn new() -> Self {
        BatchStatistics {
            total_batches: 0,
            successful_batches: 0,
            total_mutations: 0,
            passed_mutations: 0,
            avg_batch_size: 0,
            total_time_ms: 0,
        }
    }

    /// Update with new batch
    pub fn record_batch(&mut self, batch: &EvolutionBatch, duration_ms: u32) {
        self.total_batches += 1;
        if batch.status == EvolutionBatchStatus::Complete {
            self.successful_batches += 1;
        }

        let mutations_in_batch = batch.count() as u32;
        self.total_mutations += mutations_in_batch;

        let mut passed_in_batch = 0u32;
        for result in &batch.results {
            if let Some(res) = result {
                if res.passed {
                    passed_in_batch += 1;
                }
            }
        }
        self.passed_mutations += passed_in_batch;

        // Update rolling average batch size
        if self.total_batches > 0 {
            let total_size = self.total_mutations;
            self.avg_batch_size =
                total_size / self.total_batches.max(1);
        }

        self.total_time_ms = self.total_time_ms.saturating_add(duration_ms);
    }

    /// Get success rate percent
    pub fn success_rate(&self) -> u32 {
        if self.total_batches == 0 {
            return 0;
        }
        ((self.successful_batches as u64 * 100) / self.total_batches as u64) as u32
    }

    /// Get mutation pass rate percent
    pub fn mutation_pass_rate(&self) -> u32 {
        if self.total_mutations == 0 {
            return 0;
        }
        ((self.passed_mutations as u64 * 100) / self.total_mutations as u64) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_id_creation() {
        let id = BatchId::new(42);
        assert_eq!(id.value(), 42);
    }

    #[test]
    fn test_batch_id_ordering() {
        let id1 = BatchId::new(10);
        let id2 = BatchId::new(20);
        assert!(id1 < id2);
    }

    #[test]
    fn test_batched_mutation_creation() {
        let mutation = BatchedMutation::new(1, BatchId::new(100), 1000, 512);
        assert_eq!(mutation.mutation_id, 1);
        assert_eq!(mutation.estimated_cycles, 1000);
        assert_eq!(mutation.estimated_memory, 512);
    }

    #[test]
    fn test_batched_mutation_dependency() {
        let mut mutation = BatchedMutation::new(1, BatchId::new(100), 1000, 512);
        mutation.add_dependency(3);
        assert!(mutation.depends_on(3));
        assert!(!mutation.depends_on(2));
    }

    #[test]
    fn test_mutation_result_creation() {
        let result = MutationResult::new(1, true);
        assert_eq!(result.mutation_id, 1);
        assert!(result.passed);
    }

    #[test]
    fn test_mutation_batch_creation() {
        let batch = MutationBatch::new(BatchId::new(1));
        assert_eq!(batch.id, BatchId::new(1));
        assert_eq!(batch.count(), 0);
        assert_eq!(batch.status, BatchStatus::Queued);
    }

    #[test]
    fn test_mutation_batch_add_mutation() {
        let mut batch = MutationBatch::new(BatchId::new(1));
        let mutation = BatchedMutation::new(1, BatchId::new(1), 1000, 512);
        assert!(batch.add_mutation(mutation));
        assert_eq!(batch.count(), 1);
    }

    #[test]
    fn test_mutation_batch_record_result() {
        let mut batch = MutationBatch::new(BatchId::new(1));
        let mutation = BatchedMutation::new(1, BatchId::new(1), 1000, 512);
        batch.add_mutation(mutation);

        let result = MutationResult::new(1, true);
        assert!(batch.record_result(result));
    }

    #[test]
    fn test_mutation_batch_get_result() {
        let mut batch = MutationBatch::new(BatchId::new(1));
        let mutation = BatchedMutation::new(1, BatchId::new(1), 1000, 512);
        batch.add_mutation(mutation);

        let result = MutationResult::new(1, true);
        batch.record_result(result);

        assert_eq!(batch.get_result(1), Some(result));
        assert_eq!(batch.get_result(999), None);
    }

    #[test]
    fn test_mutation_batch_pass_rate() {
        let mut batch = MutationBatch::new(BatchId::new(1));
        batch.add_mutation(BatchedMutation::new(1, BatchId::new(1), 1000, 512));
        batch.add_mutation(BatchedMutation::new(2, BatchId::new(1), 1000, 512));

        batch.record_result(MutationResult::new(1, true));
        batch.record_result(MutationResult::new(2, false));

        assert_eq!(batch.pass_rate(), 50);
    }

    #[test]
    fn test_mutation_batch_avg_improvement() {
        let mut batch = MutationBatch::new(BatchId::new(1));
        batch.add_mutation(BatchedMutation::new(1, BatchId::new(1), 1000, 512));

        let mut result = MutationResult::new(1, true);
        result.improvement = 200; // 2% improvement
        batch.record_result(result);

        assert_eq!(batch.avg_improvement(), 200);
    }

    #[test]
    fn test_mutation_batch_status_transitions() {
        let mut batch = MutationBatch::new(BatchId::new(1));
        assert_eq!(batch.status, EvolutionBatchStatus::Queued);

        batch.start_execution();
        assert_eq!(batch.status, EvolutionBatchStatus::Executing);

        batch.complete(true);
        assert_eq!(batch.status, EvolutionBatchStatus::Complete);
    }

    #[test]
    fn test_parallel_test_runner_creation() {
        let runner = ParallelTestRunner::new(8);
        assert!(runner.can_run_more());
        assert_eq!(runner.pass_rate(), 0);
    }

    #[test]
    fn test_parallel_test_runner_capacity() {
        let mut runner = ParallelTestRunner::new(2);
        assert!(runner.start_mutation());
        assert!(runner.start_mutation());
        assert!(!runner.start_mutation());
    }

    #[test]
    fn test_parallel_test_runner_pass_rate() {
        let mut runner = ParallelTestRunner::new(8);
        runner.start_mutation();
        runner.finish_mutation(true);
        runner.start_mutation();
        runner.finish_mutation(false);

        assert_eq!(runner.pass_rate(), 50);
    }

    #[test]
    fn test_parallel_test_runner_utilization() {
        let mut runner = ParallelTestRunner::new(4);
        runner.start_mutation();
        runner.start_mutation();
        assert_eq!(runner.utilization(), 50);
    }

    #[test]
    fn test_adaptive_batcher_creation() {
        let batcher = AdaptiveBatcher::new(2, 8);
        assert_eq!(batcher.recommend_size(), 2);
    }

    #[test]
    fn test_adaptive_batcher_size_increase() {
        let mut batcher = AdaptiveBatcher::new(2, 8);
        batcher.update_stats(8000, 1000); // High success, high improvement
        assert!(batcher.recommend_size() > 2);
    }

    #[test]
    fn test_adaptive_batcher_size_decrease() {
        let mut batcher = AdaptiveBatcher::new(2, 8);
        batcher.current_size = 8;
        batcher.update_stats(2000, 100); // Low success
        assert!(batcher.recommend_size() < 8);
    }

    #[test]
    fn test_batch_statistics_creation() {
        let stats = BatchStatistics::new();
        assert_eq!(stats.total_batches, 0);
        assert_eq!(stats.success_rate(), 0);
    }

    #[test]
    fn test_batch_statistics_record_batch() {
        let mut batch = MutationBatch::new(BatchId::new(1));
        batch.add_mutation(BatchedMutation::new(1, BatchId::new(1), 1000, 512));
        batch.record_result(MutationResult::new(1, true));
        batch.complete(true);

        let mut stats = BatchStatistics::new();
        stats.record_batch(&batch, 100);

        assert_eq!(stats.total_batches, 1);
        assert_eq!(stats.successful_batches, 1);
    }

    #[test]
    fn test_batch_statistics_success_rate() {
        let mut batch = MutationBatch::new(BatchId::new(1));
        batch.complete(true);

        let mut stats = BatchStatistics::new();
        stats.record_batch(&batch, 100);

        assert_eq!(stats.success_rate(), 100);
    }

    #[test]
    fn test_batching_integration() {
        // Create batch
        let mut batch = MutationBatch::new(BatchId::new(1));

        // Add mutations
        for i in 1..=4 {
            let mut mutation = BatchedMutation::new(i, BatchId::new(1), 1000, 256);
            if i > 1 {
                mutation.add_dependency((i - 2) as usize);
            }
            batch.add_mutation(mutation);
        }

        // Run mutations in parallel
        let mut runner = ParallelTestRunner::new(4);
        for _ in 1..=batch.count() {
            runner.start_mutation();
        }

        // Record results
        batch.record_result(MutationResult::new(1, true));
        batch.record_result(MutationResult::new(2, true));
        batch.record_result(MutationResult::new(3, false));
        batch.record_result(MutationResult::new(4, true));

        batch.complete(true);

        // Check statistics
        assert_eq!(batch.pass_rate(), 75);
        assert_eq!(runner.pass_rate(), 75);
    }
}
