use crate::types::{LogicRay, TaskResult};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::time::Instant;

use crossbeam::queue::SegQueue;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskStage {
    Queued,
    Dispatched,
    Completed,
}

#[derive(Debug, Clone)]
struct TaskRecord {
    submitted_at: Instant,
    dispatched_at: Option<Instant>,
    completed_at: Option<Instant>,
    stage: TaskStage,
}

#[derive(Debug, Clone)]
pub struct TaskCompletion {
    pub task_id: u64,
    pub result: TaskResult,
    pub latency_us: u64,
}

/// A minimal in-kernel task queue that tracks submission → scheduling → completion.
///
/// This is intentionally simple and lock-light:
/// - `SegQueue` for pending rays (multi-producer)
/// - `SegQueue` for completions (multi-producer)
/// - `Mutex<HashMap<..>>` for per-task timestamps/state
pub struct TaskQueue<T> {
    pending_high: SegQueue<T>,
    pending_normal: SegQueue<T>,
    pending_low: SegQueue<T>,
    completed: SegQueue<TaskCompletion>,
    records: Mutex<HashMap<u64, TaskRecord>>,
}

impl<T> Default for TaskQueue<T>
where
    T: Copy + TaskMeta,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<T> TaskQueue<T>
where
    T: Copy + TaskMeta,
{
    pub fn new() -> Self {
        Self {
            pending_high: SegQueue::new(),
            pending_normal: SegQueue::new(),
            pending_low: SegQueue::new(),
            completed: SegQueue::new(),
            records: Mutex::new(HashMap::new()),
        }
    }

    pub fn submit(&self, task: T) {
        let task_id = task.task_id();
        let mut records = self.records.lock();
        records.entry(task_id).or_insert(TaskRecord {
            submitted_at: Instant::now(),
            dispatched_at: None,
            completed_at: None,
            stage: TaskStage::Queued,
        });
        drop(records);

        match bucket(task.priority()) {
            Bucket::High => self.pending_high.push(task),
            Bucket::Normal => self.pending_normal.push(task),
            Bucket::Low => self.pending_low.push(task),
        }
    }

    /// Re-queue a task that was dispatched but did not complete (e.g. GPU chunking limit).
    ///
    /// Keeps the original `submitted_at` for end-to-end latency.
    pub fn requeue(&self, task: T) {
        let task_id = task.task_id();
        let mut records = self.records.lock();
        if let Some(rec) = records.get_mut(&task_id) {
            rec.stage = TaskStage::Queued;
        }
        drop(records);

        match bucket(task.priority()) {
            Bucket::High => self.pending_high.push(task),
            Bucket::Normal => self.pending_normal.push(task),
            Bucket::Low => self.pending_low.push(task),
        }
    }

    /// Pop a task for scheduling onto System 1 workers.
    pub fn pop_for_dispatch(&self) -> Option<T> {
        let task = self
            .pending_high
            .pop()
            .or_else(|| self.pending_normal.pop())
            .or_else(|| self.pending_low.pop())?;
        let task_id = task.task_id();
        let mut records = self.records.lock();
        if let Some(rec) = records.get_mut(&task_id) {
            rec.stage = TaskStage::Dispatched;
            rec.dispatched_at = Some(Instant::now());
        }
        Some(task)
    }

    /// Mark a task as completed.
    pub fn complete(&self, task_id: u64, result: TaskResult) -> u64 {
        let now = Instant::now();
        let mut latency_us: u64 = 0;
        let mut records = self.records.lock();
        if let Some(rec) = records.get_mut(&task_id) {
            rec.stage = TaskStage::Completed;
            rec.completed_at = Some(now);
            latency_us = now
                .duration_since(rec.submitted_at)
                .as_micros()
                .try_into()
                .unwrap_or(u64::MAX);
        }
        drop(records);

        self.completed.push(TaskCompletion {
            task_id,
            result,
            latency_us,
        });

        latency_us
    }

    pub fn pending_len(&self) -> usize {
        self.pending_high.len() + self.pending_normal.len() + self.pending_low.len()
    }

    pub fn try_pop_completion(&self) -> Option<TaskCompletion> {
        self.completed.pop()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bucket {
    High,
    Normal,
    Low,
}

fn bucket(priority: u8) -> Bucket {
    if priority >= 192 {
        Bucket::High
    } else if priority >= 128 {
        Bucket::Normal
    } else {
        Bucket::Low
    }
}

pub trait TaskMeta {
    fn task_id(&self) -> u64;
    fn priority(&self) -> u8;
}

impl TaskMeta for LogicRay {
    fn task_id(&self) -> u64 {
        self.task_id
    }

    fn priority(&self) -> u8 {
        self.priority
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct DummyTask {
        id: u64,
    }

    impl TaskMeta for DummyTask {
        fn task_id(&self) -> u64 {
            self.id
        }

        fn priority(&self) -> u8 {
            0
        }
    }

    #[test]
    fn submit_dispatch_complete_roundtrip() {
        let q: TaskQueue<DummyTask> = TaskQueue::new();
        q.submit(DummyTask { id: 1 });
        q.submit(DummyTask { id: 2 });

        assert_eq!(q.pending_len(), 2);

        let t1 = q.pop_for_dispatch().unwrap();
        assert!(t1.id == 1 || t1.id == 2);
        let t2 = q.pop_for_dispatch().unwrap();
        assert!(t2.id != t1.id);

        assert_eq!(q.pending_len(), 0);

        q.complete(t1.id, TaskResult::Success);
        q.complete(t2.id, TaskResult::Retry);

        let mut seen = 0;
        while let Some(c) = q.try_pop_completion() {
            assert!(c.task_id == 1 || c.task_id == 2);
            seen += 1;
        }
        assert_eq!(seen, 2);
    }

    #[test]
    fn priority_scheduling_prefers_high() {
        #[derive(Clone, Copy)]
        struct PTask {
            id: u64,
            pri: u8,
        }

        impl TaskMeta for PTask {
            fn task_id(&self) -> u64 {
                self.id
            }

            fn priority(&self) -> u8 {
                self.pri
            }
        }

        let q: TaskQueue<PTask> = TaskQueue::new();
        q.submit(PTask { id: 1, pri: 0 });
        q.submit(PTask { id: 2, pri: 255 });
        q.submit(PTask { id: 3, pri: 128 });

        let t = q.pop_for_dispatch().unwrap();
        assert_eq!(t.id, 2);
    }
}
