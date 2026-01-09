//! Pipeline Stages - Stage types and input/output definitions
//!
//! This module defines the high-level interface for submitting work to
//! the unified pipeline. Each stage type has its own input and output
//! structures that map to the underlying GPU work items.

use std::fmt;

// =============================================================================
// Stage Types
// =============================================================================

/// The type of pipeline stage
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StageType {
    /// Perception: visual/sensor processing
    Perception,
    /// Logic: decision making, access control
    Logic,
    /// Semantic: embedding similarity, KNN
    Semantic,
    /// Custom: user-defined computation
    Custom,
}

impl StageType {
    /// Get the GPU stage type ID
    pub fn as_gpu_id(&self) -> u32 {
        match self {
            StageType::Perception => 0,
            StageType::Logic => 1,
            StageType::Semantic => 2,
            StageType::Custom => 3,
        }
    }

    /// Create from GPU stage type ID
    pub fn from_gpu_id(id: u32) -> Option<Self> {
        match id {
            0 => Some(StageType::Perception),
            1 => Some(StageType::Logic),
            2 => Some(StageType::Semantic),
            3 => Some(StageType::Custom),
            _ => None,
        }
    }
}

impl fmt::Display for StageType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StageType::Perception => write!(f, "Perception"),
            StageType::Logic => write!(f, "Logic"),
            StageType::Semantic => write!(f, "Semantic"),
            StageType::Custom => write!(f, "Custom"),
        }
    }
}

// =============================================================================
// Pipeline Stage Trait
// =============================================================================

/// A stage in the unified pipeline
pub trait PipelineStage: Send + Sync {
    /// Get the stage type
    fn stage_type(&self) -> StageType;

    /// Get the number of work items this stage will generate
    fn work_item_count(&self) -> usize;

    /// Encode inputs into the shared input buffer
    /// Returns (work_items, input_floats)
    fn encode(&self, base_input_offset: u32, base_output_offset: u32) -> (Vec<super::core::WorkItem>, Vec<f32>);
}

// =============================================================================
// Perception Stage
// =============================================================================

/// Input for perception stage
#[derive(Clone, Debug)]
pub struct PerceptionInput {
    /// Window of intensity values to process
    pub window: Vec<f32>,
    /// Expected baseline (for motion detection)
    pub baseline: f32,
}

impl PerceptionInput {
    /// Create a new perception input
    pub fn new(window: Vec<f32>, baseline: f32) -> Self {
        Self { window, baseline }
    }

    /// Create from raw pixel data
    pub fn from_pixels(pixels: &[u8], baseline: f32) -> Self {
        let window: Vec<f32> = pixels.iter().map(|&p| p as f32 / 255.0).collect();
        Self { window, baseline }
    }
}

/// Output from perception stage
#[derive(Clone, Debug)]
pub struct PerceptionOutput {
    /// Average intensity
    pub average_intensity: f32,
    /// Motion magnitude
    pub motion_magnitude: f32,
    /// Whether motion was detected (above threshold)
    pub motion_detected: bool,
}

impl PerceptionOutput {
    /// Motion detection threshold
    pub const MOTION_THRESHOLD: f32 = 0.1;

    /// Create from work result
    pub fn from_result(primary: f32, secondary: f32) -> Self {
        Self {
            average_intensity: primary,
            motion_magnitude: secondary,
            motion_detected: secondary > Self::MOTION_THRESHOLD,
        }
    }
}

/// Perception stage implementation
#[derive(Clone, Debug)]
pub struct PerceptionStage {
    inputs: Vec<PerceptionInput>,
}

impl PerceptionStage {
    /// Create a new perception stage
    pub fn new() -> Self {
        Self { inputs: Vec::new() }
    }

    /// Add a perception input
    pub fn add(&mut self, input: PerceptionInput) {
        self.inputs.push(input);
    }

    /// Add multiple perception inputs
    pub fn add_batch(&mut self, inputs: impl IntoIterator<Item = PerceptionInput>) {
        self.inputs.extend(inputs);
    }
}

impl Default for PerceptionStage {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineStage for PerceptionStage {
    fn stage_type(&self) -> StageType {
        StageType::Perception
    }

    fn work_item_count(&self) -> usize {
        self.inputs.len()
    }

    fn encode(&self, base_input_offset: u32, base_output_offset: u32) -> (Vec<super::core::WorkItem>, Vec<f32>) {
        let mut work_items = Vec::with_capacity(self.inputs.len());
        let mut input_data = Vec::new();

        for (i, input) in self.inputs.iter().enumerate() {
            let input_offset = base_input_offset + input_data.len() as u32;
            let output_offset = base_output_offset + i as u32;
            let window_size = input.window.len() as u32;

            work_items.push(super::core::WorkItem::perception(
                input_offset,
                output_offset,
                window_size,
            ));

            // Encode window data
            input_data.extend_from_slice(&input.window);
            // Encode baseline after window
            input_data.push(input.baseline);
        }

        (work_items, input_data)
    }
}

// =============================================================================
// Logic Stage
// =============================================================================

/// Input for logic stage (access control)
#[derive(Clone, Debug)]
pub struct LogicInput {
    /// User ID
    pub user_id: u32,
    /// Resource ID
    pub resource_id: u32,
    /// Requested permission bits
    pub requested_permission: u32,
    /// Resource owner ID
    pub owner_id: u32,
    /// Resource permission bits
    pub resource_permissions: u32,
}

impl LogicInput {
    /// Create a new logic input
    pub fn new(
        user_id: u32,
        resource_id: u32,
        requested_permission: u32,
        owner_id: u32,
        resource_permissions: u32,
    ) -> Self {
        Self {
            user_id,
            resource_id,
            requested_permission,
            owner_id,
            resource_permissions,
        }
    }

    /// Create an access check query
    pub fn access_check(user_id: u32, resource_id: u32, permission: u32) -> Self {
        Self {
            user_id,
            resource_id,
            requested_permission: permission,
            owner_id: 0,
            resource_permissions: 0,
        }
    }

    /// Set resource info
    pub fn with_resource_info(mut self, owner_id: u32, permissions: u32) -> Self {
        self.owner_id = owner_id;
        self.resource_permissions = permissions;
        self
    }

    /// Mark user as admin (sets MSB)
    pub fn as_admin(mut self) -> Self {
        self.user_id |= 0x80000000;
        self
    }
}

/// Output from logic stage
#[derive(Clone, Debug)]
pub struct LogicOutput {
    /// Access granted
    pub granted: bool,
    /// Confidence level (0.0 - 1.0)
    pub confidence: f32,
}

impl LogicOutput {
    /// Create from work result
    pub fn from_result(primary: f32, secondary: f32) -> Self {
        Self {
            granted: primary > 0.5,
            confidence: secondary,
        }
    }
}

/// Logic stage implementation
#[derive(Clone, Debug)]
pub struct LogicStage {
    inputs: Vec<LogicInput>,
}

impl LogicStage {
    /// Create a new logic stage
    pub fn new() -> Self {
        Self { inputs: Vec::new() }
    }

    /// Add a logic input
    pub fn add(&mut self, input: LogicInput) {
        self.inputs.push(input);
    }

    /// Add multiple logic inputs
    pub fn add_batch(&mut self, inputs: impl IntoIterator<Item = LogicInput>) {
        self.inputs.extend(inputs);
    }
}

impl Default for LogicStage {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineStage for LogicStage {
    fn stage_type(&self) -> StageType {
        StageType::Logic
    }

    fn work_item_count(&self) -> usize {
        self.inputs.len()
    }

    fn encode(&self, base_input_offset: u32, base_output_offset: u32) -> (Vec<super::core::WorkItem>, Vec<f32>) {
        let mut work_items = Vec::with_capacity(self.inputs.len());
        let mut input_data = Vec::new();

        for (i, input) in self.inputs.iter().enumerate() {
            let input_offset = base_input_offset + input_data.len() as u32;
            let output_offset = base_output_offset + i as u32;

            work_items.push(super::core::WorkItem::logic(input_offset, output_offset));

            // Encode logic input as 5 floats (bitcast from u32)
            input_data.push(f32::from_bits(input.user_id));
            input_data.push(f32::from_bits(input.resource_id));
            input_data.push(f32::from_bits(input.requested_permission));
            input_data.push(f32::from_bits(input.owner_id));
            input_data.push(f32::from_bits(input.resource_permissions));
        }

        (work_items, input_data)
    }
}

// =============================================================================
// Semantic Stage
// =============================================================================

/// Input for semantic stage (vector similarity)
#[derive(Clone, Debug)]
pub struct SemanticInput {
    /// First vector (query)
    pub vector_a: Vec<f32>,
    /// Second vector (database entry)
    pub vector_b: Vec<f32>,
}

impl SemanticInput {
    /// Create a new semantic input
    pub fn new(vector_a: Vec<f32>, vector_b: Vec<f32>) -> Self {
        debug_assert_eq!(vector_a.len(), vector_b.len(), "Vector dimensions must match");
        Self { vector_a, vector_b }
    }

    /// Create a similarity query
    pub fn similarity_query(query: Vec<f32>, candidate: Vec<f32>) -> Self {
        Self::new(query, candidate)
    }
}

/// Output from semantic stage
#[derive(Clone, Debug)]
pub struct SemanticOutput {
    /// Cosine similarity (-1.0 to 1.0)
    pub similarity: f32,
    /// Raw dot product
    pub dot_product: f32,
}

impl SemanticOutput {
    /// Create from work result
    pub fn from_result(primary: f32, secondary: f32) -> Self {
        Self {
            similarity: primary,
            dot_product: secondary,
        }
    }

    /// Check if similarity is above threshold
    pub fn is_similar(&self, threshold: f32) -> bool {
        self.similarity >= threshold
    }
}

/// Semantic stage implementation
#[derive(Clone, Debug)]
pub struct SemanticStage {
    inputs: Vec<SemanticInput>,
}

impl SemanticStage {
    /// Create a new semantic stage
    pub fn new() -> Self {
        Self { inputs: Vec::new() }
    }

    /// Add a semantic input
    pub fn add(&mut self, input: SemanticInput) {
        self.inputs.push(input);
    }

    /// Add multiple semantic inputs
    pub fn add_batch(&mut self, inputs: impl IntoIterator<Item = SemanticInput>) {
        self.inputs.extend(inputs);
    }
}

impl Default for SemanticStage {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineStage for SemanticStage {
    fn stage_type(&self) -> StageType {
        StageType::Semantic
    }

    fn work_item_count(&self) -> usize {
        self.inputs.len()
    }

    fn encode(&self, base_input_offset: u32, base_output_offset: u32) -> (Vec<super::core::WorkItem>, Vec<f32>) {
        let mut work_items = Vec::with_capacity(self.inputs.len());
        let mut input_data = Vec::new();

        for (i, input) in self.inputs.iter().enumerate() {
            let input_offset = base_input_offset + input_data.len() as u32;
            let output_offset = base_output_offset + i as u32;
            let dim = input.vector_a.len() as u32;

            work_items.push(super::core::WorkItem::semantic(
                input_offset,
                output_offset,
                dim,
            ));

            // Encode both vectors consecutively
            input_data.extend_from_slice(&input.vector_a);
            input_data.extend_from_slice(&input.vector_b);
        }

        (work_items, input_data)
    }
}

// =============================================================================
// Custom Stage
// =============================================================================

/// Operation codes for custom stage
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomOp {
    /// Sum operation
    Sum = 0,
    /// Max operation
    Max = 1,
    /// Threshold operation
    Threshold = 2,
}

impl CustomOp {
    /// Get the GPU op code
    pub fn as_gpu_code(&self) -> u32 {
        *self as u32
    }
}

/// Input for custom stage
#[derive(Clone, Debug)]
pub struct CustomInput {
    /// Operation to perform
    pub op: CustomOp,
    /// Input data
    pub data: Vec<f32>,
}

impl CustomInput {
    /// Create a new custom input
    pub fn new(op: CustomOp, data: Vec<f32>) -> Self {
        Self { op, data }
    }

    /// Create a sum operation
    pub fn sum(values: Vec<f32>) -> Self {
        let mut data = vec![values.len() as f32];
        data.extend(values);
        Self { op: CustomOp::Sum, data }
    }

    /// Create a max operation
    pub fn max(values: Vec<f32>) -> Self {
        let mut data = vec![values.len() as f32];
        data.extend(values);
        Self { op: CustomOp::Max, data }
    }

    /// Create a threshold operation
    pub fn threshold(value: f32, threshold: f32) -> Self {
        Self {
            op: CustomOp::Threshold,
            data: vec![value, threshold],
        }
    }
}

/// Output from custom stage
#[derive(Clone, Debug)]
pub struct CustomOutput {
    /// Primary result
    pub result: f32,
    /// Auxiliary result
    pub aux: f32,
}

impl CustomOutput {
    /// Create from work result
    pub fn from_result(primary: f32, secondary: f32) -> Self {
        Self {
            result: primary,
            aux: secondary,
        }
    }
}

/// Custom stage implementation
#[derive(Clone, Debug)]
pub struct CustomStage {
    inputs: Vec<CustomInput>,
}

impl CustomStage {
    /// Create a new custom stage
    pub fn new() -> Self {
        Self { inputs: Vec::new() }
    }

    /// Add a custom input
    pub fn add(&mut self, input: CustomInput) {
        self.inputs.push(input);
    }

    /// Add multiple custom inputs
    pub fn add_batch(&mut self, inputs: impl IntoIterator<Item = CustomInput>) {
        self.inputs.extend(inputs);
    }
}

impl Default for CustomStage {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineStage for CustomStage {
    fn stage_type(&self) -> StageType {
        StageType::Custom
    }

    fn work_item_count(&self) -> usize {
        self.inputs.len()
    }

    fn encode(&self, base_input_offset: u32, base_output_offset: u32) -> (Vec<super::core::WorkItem>, Vec<f32>) {
        let mut work_items = Vec::with_capacity(self.inputs.len());
        let mut input_data = Vec::new();

        for (i, input) in self.inputs.iter().enumerate() {
            let input_offset = base_input_offset + input_data.len() as u32;
            let output_offset = base_output_offset + i as u32;

            work_items.push(super::core::WorkItem::custom(
                input_offset,
                output_offset,
                input.op.as_gpu_code(),
            ));

            input_data.extend_from_slice(&input.data);
        }

        (work_items, input_data)
    }
}

// =============================================================================
// Stage Input/Output Enums
// =============================================================================

/// Input to any pipeline stage
#[derive(Clone, Debug)]
pub enum StageInput {
    /// Perception input
    Perception(PerceptionInput),
    /// Logic input
    Logic(LogicInput),
    /// Semantic input
    Semantic(SemanticInput),
    /// Custom input
    Custom(CustomInput),
}

impl StageInput {
    /// Get the stage type
    pub fn stage_type(&self) -> StageType {
        match self {
            StageInput::Perception(_) => StageType::Perception,
            StageInput::Logic(_) => StageType::Logic,
            StageInput::Semantic(_) => StageType::Semantic,
            StageInput::Custom(_) => StageType::Custom,
        }
    }
}

/// Output from any pipeline stage
#[derive(Clone, Debug)]
pub enum StageOutput {
    /// Perception output
    Perception(PerceptionOutput),
    /// Logic output
    Logic(LogicOutput),
    /// Semantic output
    Semantic(SemanticOutput),
    /// Custom output
    Custom(CustomOutput),
}

impl StageOutput {
    /// Get the stage type
    pub fn stage_type(&self) -> StageType {
        match self {
            StageOutput::Perception(_) => StageType::Perception,
            StageOutput::Logic(_) => StageType::Logic,
            StageOutput::Semantic(_) => StageType::Semantic,
            StageOutput::Custom(_) => StageType::Custom,
        }
    }

    /// Create from work result
    pub fn from_result(stage_type: u32, primary: f32, secondary: f32) -> Option<Self> {
        match stage_type {
            0 => Some(StageOutput::Perception(PerceptionOutput::from_result(primary, secondary))),
            1 => Some(StageOutput::Logic(LogicOutput::from_result(primary, secondary))),
            2 => Some(StageOutput::Semantic(SemanticOutput::from_result(primary, secondary))),
            3 => Some(StageOutput::Custom(CustomOutput::from_result(primary, secondary))),
            _ => None,
        }
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_type_conversion() {
        assert_eq!(StageType::Perception.as_gpu_id(), 0);
        assert_eq!(StageType::Logic.as_gpu_id(), 1);
        assert_eq!(StageType::Semantic.as_gpu_id(), 2);
        assert_eq!(StageType::Custom.as_gpu_id(), 3);

        assert_eq!(StageType::from_gpu_id(0), Some(StageType::Perception));
        assert_eq!(StageType::from_gpu_id(1), Some(StageType::Logic));
        assert_eq!(StageType::from_gpu_id(2), Some(StageType::Semantic));
        assert_eq!(StageType::from_gpu_id(3), Some(StageType::Custom));
        assert_eq!(StageType::from_gpu_id(4), None);
    }

    #[test]
    fn test_perception_input() {
        let input = PerceptionInput::new(vec![0.5, 0.6, 0.7], 0.5);
        assert_eq!(input.window.len(), 3);
        assert_eq!(input.baseline, 0.5);

        let from_pixels = PerceptionInput::from_pixels(&[128, 255, 0], 0.5);
        assert!((from_pixels.window[0] - 0.502).abs() < 0.01);
        assert!((from_pixels.window[1] - 1.0).abs() < 0.01);
        assert!((from_pixels.window[2] - 0.0).abs() < 0.01);
    }

    #[test]
    fn test_perception_output() {
        let output = PerceptionOutput::from_result(0.7, 0.15);
        assert!((output.average_intensity - 0.7).abs() < 0.01);
        assert!((output.motion_magnitude - 0.15).abs() < 0.01);
        assert!(output.motion_detected);

        let no_motion = PerceptionOutput::from_result(0.5, 0.05);
        assert!(!no_motion.motion_detected);
    }

    #[test]
    fn test_perception_stage_encode() {
        let mut stage = PerceptionStage::new();
        stage.add(PerceptionInput::new(vec![0.1, 0.2, 0.3], 0.2));
        stage.add(PerceptionInput::new(vec![0.4, 0.5], 0.45));

        let (work_items, input_data) = stage.encode(0, 0);

        assert_eq!(work_items.len(), 2);
        assert_eq!(work_items[0].stage_type, 0);
        assert_eq!(work_items[0].param, 3); // window size
        assert_eq!(work_items[1].param, 2); // window size

        // First input: 3 values + baseline = 4 floats
        // Second input: 2 values + baseline = 3 floats
        assert_eq!(input_data.len(), 7);
    }

    #[test]
    fn test_logic_input() {
        let input = LogicInput::access_check(1, 2, 4)
            .with_resource_info(1, 0o644);
        assert_eq!(input.user_id, 1);
        assert_eq!(input.resource_id, 2);
        assert_eq!(input.requested_permission, 4);
        assert_eq!(input.owner_id, 1);
        assert_eq!(input.resource_permissions, 0o644);

        let admin = LogicInput::access_check(1, 2, 4).as_admin();
        assert_eq!(admin.user_id & 0x80000000, 0x80000000);
    }

    #[test]
    fn test_logic_output() {
        let granted = LogicOutput::from_result(1.0, 0.95);
        assert!(granted.granted);
        assert!((granted.confidence - 0.95).abs() < 0.01);

        let denied = LogicOutput::from_result(0.0, 0.0);
        assert!(!denied.granted);
    }

    #[test]
    fn test_logic_stage_encode() {
        let mut stage = LogicStage::new();
        stage.add(LogicInput::access_check(1, 2, 4).with_resource_info(1, 0o644));

        let (work_items, input_data) = stage.encode(0, 0);

        assert_eq!(work_items.len(), 1);
        assert_eq!(work_items[0].stage_type, 1);
        assert_eq!(input_data.len(), 5); // 5 u32s encoded as f32
    }

    #[test]
    fn test_semantic_input() {
        let input = SemanticInput::new(vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]);
        assert_eq!(input.vector_a.len(), 3);
        assert_eq!(input.vector_b.len(), 3);
    }

    #[test]
    fn test_semantic_output() {
        let output = SemanticOutput::from_result(0.85, 1.5);
        assert!((output.similarity - 0.85).abs() < 0.01);
        assert!((output.dot_product - 1.5).abs() < 0.01);
        assert!(output.is_similar(0.8));
        assert!(!output.is_similar(0.9));
    }

    #[test]
    fn test_semantic_stage_encode() {
        let mut stage = SemanticStage::new();
        stage.add(SemanticInput::new(vec![1.0, 0.0, 0.0], vec![0.0, 1.0, 0.0]));

        let (work_items, input_data) = stage.encode(0, 0);

        assert_eq!(work_items.len(), 1);
        assert_eq!(work_items[0].stage_type, 2);
        assert_eq!(work_items[0].param, 3); // vector dimension
        assert_eq!(input_data.len(), 6); // 2 vectors * 3 dims
    }

    #[test]
    fn test_custom_input_sum() {
        let input = CustomInput::sum(vec![1.0, 2.0, 3.0]);
        assert_eq!(input.op, CustomOp::Sum);
        assert_eq!(input.data.len(), 4); // count + 3 values
        assert_eq!(input.data[0], 3.0); // count
    }

    #[test]
    fn test_custom_input_threshold() {
        let input = CustomInput::threshold(0.8, 0.5);
        assert_eq!(input.op, CustomOp::Threshold);
        assert_eq!(input.data.len(), 2);
    }

    #[test]
    fn test_custom_output() {
        let output = CustomOutput::from_result(6.0, 0.0);
        assert_eq!(output.result, 6.0);
        assert_eq!(output.aux, 0.0);
    }

    #[test]
    fn test_custom_stage_encode() {
        let mut stage = CustomStage::new();
        stage.add(CustomInput::sum(vec![1.0, 2.0, 3.0]));
        stage.add(CustomInput::threshold(0.8, 0.5));

        let (work_items, input_data) = stage.encode(0, 0);

        assert_eq!(work_items.len(), 2);
        assert_eq!(work_items[0].param, 0); // Sum op code
        assert_eq!(work_items[1].param, 2); // Threshold op code
        assert_eq!(input_data.len(), 6); // 4 for sum + 2 for threshold
    }

    #[test]
    fn test_stage_input_type() {
        let perception = StageInput::Perception(PerceptionInput::new(vec![], 0.0));
        assert_eq!(perception.stage_type(), StageType::Perception);

        let logic = StageInput::Logic(LogicInput::access_check(1, 1, 1));
        assert_eq!(logic.stage_type(), StageType::Logic);
    }

    #[test]
    fn test_stage_output_from_result() {
        let perception = StageOutput::from_result(0, 0.5, 0.1);
        assert!(matches!(perception, Some(StageOutput::Perception(_))));

        let logic = StageOutput::from_result(1, 1.0, 0.9);
        assert!(matches!(logic, Some(StageOutput::Logic(_))));

        let semantic = StageOutput::from_result(2, 0.95, 1.5);
        assert!(matches!(semantic, Some(StageOutput::Semantic(_))));

        let custom = StageOutput::from_result(3, 6.0, 0.0);
        assert!(matches!(custom, Some(StageOutput::Custom(_))));

        let invalid = StageOutput::from_result(99, 0.0, 0.0);
        assert!(invalid.is_none());
    }
}
