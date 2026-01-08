/// Predictive Resource Allocation
///
/// Machine learning-style resource prediction and intelligent allocation
/// based on historical usage patterns and trend analysis.

use core::cmp::min;

const MAX_TRACKED_RESOURCES: usize = 32;
const MAX_HISTORY_ENTRIES: usize = 256;

/// Prediction model type
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PredictionModel {
    Linear = 0,
    Exponential = 1,
    Seasonal = 2,
}

/// Allocation policy
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AllocationPolicy {
    Conservative = 0,
    Balanced = 1,
    Aggressive = 2,
}

/// Resource pattern
#[derive(Clone, Copy, Debug)]
pub struct ResourcePattern {
    pub resource_id: u32,
    pub min_usage: u64,
    pub max_usage: u64,
    pub avg_usage: u64,
    pub current_usage: u64,
    pub trend: i32,
    pub volatility: u32,
}

impl ResourcePattern {
    pub fn new(resource_id: u32) -> Self {
        ResourcePattern {
            resource_id,
            min_usage: 0,
            max_usage: 0,
            avg_usage: 0,
            current_usage: 0,
            trend: 0,
            volatility: 0,
        }
    }
}

/// History entry for resource
#[derive(Clone, Copy, Debug)]
pub struct HistoryEntry {
    pub timestamp: u64,
    pub usage: u64,
}

impl HistoryEntry {
    pub fn new(timestamp: u64, usage: u64) -> Self {
        HistoryEntry { timestamp, usage }
    }
}

/// Resource forecast
#[derive(Clone, Copy, Debug)]
pub struct ResourceForecast {
    pub resource_id: u32,
    pub predicted_usage: u64,
    pub confidence: u32,
    pub time_horizon: u32,
}

impl ResourceForecast {
    pub fn new(resource_id: u32, predicted: u64, confidence: u32) -> Self {
        ResourceForecast {
            resource_id,
            predicted_usage: predicted,
            confidence,
            time_horizon: 60,
        }
    }
}

/// Resource Predictor
pub struct ResourcePredictor {
    patterns: [Option<ResourcePattern>; MAX_TRACKED_RESOURCES],
    history: [Option<HistoryEntry>; MAX_HISTORY_ENTRIES],
    model: PredictionModel,
    policy: AllocationPolicy,
    resource_count: u32,
    history_index: u32,
}

impl ResourcePredictor {
    pub fn new(model: PredictionModel, policy: AllocationPolicy) -> Self {
        ResourcePredictor {
            patterns: [None; MAX_TRACKED_RESOURCES],
            history: [None; MAX_HISTORY_ENTRIES],
            model,
            policy,
            resource_count: 0,
            history_index: 0,
        }
    }

    pub fn track_resource(&mut self, resource_id: u32) -> bool {
        if self.resource_count >= MAX_TRACKED_RESOURCES as u32 {
            return false;
        }

        for i in 0..MAX_TRACKED_RESOURCES {
            if self.patterns[i].is_none() {
                let pattern = ResourcePattern::new(resource_id);
                self.patterns[i] = Some(pattern);
                self.resource_count += 1;
                return true;
            }
        }
        false
    }

    pub fn record_usage(&mut self, resource_id: u32, usage: u64, timestamp: u64) -> bool {
        let idx = (self.history_index as usize) % MAX_HISTORY_ENTRIES;
        let entry = HistoryEntry::new(timestamp, usage);
        self.history[idx] = Some(entry);
        self.history_index += 1;

        // Update pattern
        for i in 0..MAX_TRACKED_RESOURCES {
            if let Some(mut pattern) = self.patterns[i] {
                if pattern.resource_id == resource_id {
                    pattern.current_usage = usage;
                    pattern.min_usage = min(pattern.min_usage, usage);
                    pattern.max_usage = if usage > pattern.max_usage {
                        usage
                    } else {
                        pattern.max_usage
                    };
                    pattern.avg_usage = (pattern.avg_usage + usage) / 2;
                    self.patterns[i] = Some(pattern);
                    return true;
                }
            }
        }
        false
    }

    pub fn predict_linear(&self, resource_id: u32) -> u64 {
        for i in 0..MAX_TRACKED_RESOURCES {
            if let Some(pattern) = self.patterns[i] {
                if pattern.resource_id == resource_id {
                    let trend_factor = (pattern.trend as i64) / 100;
                    let predicted = (pattern.current_usage as i64) + trend_factor;
                    return if predicted < 0 {
                        0
                    } else {
                        predicted as u64
                    };
                }
            }
        }
        0
    }

    pub fn predict_exponential(&self, resource_id: u32) -> u64 {
        for i in 0..MAX_TRACKED_RESOURCES {
            if let Some(pattern) = self.patterns[i] {
                if pattern.resource_id == resource_id {
                    let factor = 105; // 5% growth factor
                    return (pattern.current_usage * factor) / 100;
                }
            }
        }
        0
    }

    pub fn predict_seasonal(&self, _resource_id: u32) -> u64 {
        // Simplified seasonal prediction
        let mut avg = 0u64;
        let mut count = 0;
        for i in 0..MAX_HISTORY_ENTRIES {
            if let Some(entry) = self.history[i] {
                avg += entry.usage;
                count += 1;
            }
        }
        if count > 0 {
            avg / count as u64
        } else {
            0
        }
    }

    pub fn predict(&self, resource_id: u32) -> ResourceForecast {
        let predicted = match self.model {
            PredictionModel::Linear => self.predict_linear(resource_id),
            PredictionModel::Exponential => self.predict_exponential(resource_id),
            PredictionModel::Seasonal => self.predict_seasonal(resource_id),
        };

        let confidence = match self.policy {
            AllocationPolicy::Conservative => 60,
            AllocationPolicy::Balanced => 75,
            AllocationPolicy::Aggressive => 90,
        };

        ResourceForecast::new(resource_id, predicted, confidence)
    }

    pub fn detect_anomaly(&self, resource_id: u32, current_usage: u64) -> bool {
        for i in 0..MAX_TRACKED_RESOURCES {
            if let Some(pattern) = self.patterns[i] {
                if pattern.resource_id == resource_id {
                    let max_expected = pattern.max_usage + (pattern.volatility as u64);
                    return current_usage > max_expected;
                }
            }
        }
        false
    }

    pub fn detect_trend(&mut self, resource_id: u32) -> i32 {
        let mut trend = 0i32;
        for i in 0..MAX_TRACKED_RESOURCES {
            if let Some(mut pattern) = self.patterns[i] {
                if pattern.resource_id == resource_id {
                    if pattern.current_usage > pattern.avg_usage {
                        trend = 10;
                    } else if pattern.current_usage < pattern.avg_usage {
                        trend = -10;
                    }
                    pattern.trend = trend;
                    self.patterns[i] = Some(pattern);
                    return trend;
                }
            }
        }
        trend
    }

    pub fn allocate(&self, resource_id: u32) -> u64 {
        let forecast = self.predict(resource_id);
        match self.policy {
            AllocationPolicy::Conservative => forecast.predicted_usage,
            AllocationPolicy::Balanced => (forecast.predicted_usage * 115) / 100,
            AllocationPolicy::Aggressive => (forecast.predicted_usage * 150) / 100,
        }
    }

    pub fn get_resource_count(&self) -> u32 {
        self.resource_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_tracking() {
        let mut predictor = ResourcePredictor::new(PredictionModel::Linear, AllocationPolicy::Balanced);
        assert!(predictor.track_resource(1));
        assert_eq!(predictor.get_resource_count(), 1);
    }

    #[test]
    fn test_linear_prediction() {
        let mut predictor = ResourcePredictor::new(PredictionModel::Linear, AllocationPolicy::Balanced);
        predictor.track_resource(1);
        predictor.record_usage(1, 100, 1000);
        let predicted = predictor.predict_linear(1);
        assert!(predicted >= 0);
    }

    #[test]
    fn test_exponential_smoothing() {
        let mut predictor = ResourcePredictor::new(PredictionModel::Exponential, AllocationPolicy::Balanced);
        predictor.track_resource(1);
        predictor.record_usage(1, 100, 1000);
        let predicted = predictor.predict_exponential(1);
        assert!(predicted > 100);
    }

    #[test]
    fn test_seasonal_patterns() {
        let mut predictor = ResourcePredictor::new(PredictionModel::Seasonal, AllocationPolicy::Balanced);
        predictor.track_resource(1);
        for i in 0..10 {
            predictor.record_usage(1, 100 + i * 10, 1000 + i as u64);
        }
        let predicted = predictor.predict_seasonal(1);
        assert!(predicted > 0);
    }

    #[test]
    fn test_allocation_policy() {
        let mut predictor = ResourcePredictor::new(PredictionModel::Linear, AllocationPolicy::Conservative);
        predictor.track_resource(1);
        predictor.record_usage(1, 100, 1000);
        let allocated = predictor.allocate(1);
        assert!(allocated > 0);
    }

    #[test]
    fn test_trend_detection() {
        let mut predictor = ResourcePredictor::new(PredictionModel::Linear, AllocationPolicy::Balanced);
        predictor.track_resource(1);
        predictor.record_usage(1, 100, 1000);
        predictor.record_usage(1, 150, 2000);
        let trend = predictor.detect_trend(1);
        assert!(trend > 0);
    }

    #[test]
    fn test_anomaly_detection() {
        let mut predictor = ResourcePredictor::new(PredictionModel::Linear, AllocationPolicy::Balanced);
        predictor.track_resource(1);
        predictor.record_usage(1, 100, 1000);
        let is_anomaly = predictor.detect_anomaly(1, 10000);
        assert!(is_anomaly);
    }

    #[test]
    fn test_forecast_accuracy() {
        let predictor = ResourcePredictor::new(PredictionModel::Linear, AllocationPolicy::Balanced);
        let forecast = predictor.predict(1);
        assert!(forecast.confidence > 0);
    }
}
