//! Object recognition for visual context understanding
//!
//! This module identifies objects in the visual field to provide
//! context for the LLM (e.g., "user is holding a coffee cup").

use crate::types::{DetectedObject, BoundingBox};
#[cfg(feature = "vision")]
use opencv::{core, dnn, prelude::*};
use anyhow::{Context, Result};
use std::path::Path;

pub struct ObjectRecognizer {
    // Placeholder for ML model
    // In production, this would load YOLO, MobileNet, or similar
    #[cfg(feature = "vision")]
    net: Option<dnn::Net>,
    #[cfg(feature = "vision")]
    ssd_input_size: core::Size,
    classes: Vec<String>,
}

impl ObjectRecognizer {
    pub async fn new() -> Result<Self> {
        log::info!("Initializing Object Recognizer...");

        // Placeholder class list (COCO dataset classes)
        let classes = vec![
            "person", "bicycle", "car", "motorcycle", "airplane",
            "bus", "train", "truck", "boat", "traffic light",
            "fire hydrant", "stop sign", "parking meter", "bench",
            "bird", "cat", "dog", "horse", "sheep", "cow",
            "elephant", "bear", "zebra", "giraffe", "backpack",
            "umbrella", "handbag", "tie", "suitcase", "frisbee",
            "skis", "snowboard", "sports ball", "kite", "baseball bat",
            "baseball glove", "skateboard", "surfboard", "tennis racket",
            "bottle", "wine glass", "cup", "fork", "knife",
            "spoon", "bowl", "banana", "apple", "sandwich",
            "orange", "broccoli", "carrot", "hot dog", "pizza",
            "donut", "cake", "chair", "couch", "potted plant",
            "bed", "dining table", "toilet", "tv", "laptop",
            "mouse", "remote", "keyboard", "cell phone", "microwave",
            "oven", "toaster", "sink", "refrigerator", "book",
            "clock", "vase", "scissors", "teddy bear", "hair drier",
            "toothbrush",
        ].iter().map(|s| s.to_string()).collect();

        #[cfg(feature = "vision")]
        {
            // Optional: load a real DNN model if paths are provided.
            // Supported today: MobileNet-SSD (Caffe) style outputs: 1x1xNx7
            // Env vars:
            // - RAYOS_DNN_SSD_PROTOTXT=/path/to/deploy.prototxt
            // - RAYOS_DNN_SSD_MODEL=/path/to/weights.caffemodel
            // - RAYOS_DNN_CLASSES=/path/to/classes.txt (optional)
            // - RAYOS_DNN_INPUT=300 (optional; default 300)

            let mut net: Option<dnn::Net> = None;
            let mut ssd_input_size = core::Size::new(300, 300);

            if let Ok(n) = std::env::var("RAYOS_DNN_INPUT") {
                if let Ok(v) = n.parse::<i32>() {
                    ssd_input_size = core::Size::new(v, v);
                }
            }

            if let (Ok(proto), Ok(model)) = (
                std::env::var("RAYOS_DNN_SSD_PROTOTXT"),
                std::env::var("RAYOS_DNN_SSD_MODEL"),
            ) {
                if Path::new(&proto).exists() && Path::new(&model).exists() {
                    log::info!("Loading DNN (SSD/Caffe): prototxt={} model={}", proto, model);
                    match dnn::read_net_from_caffe(&proto, &model) {
                        Ok(n) => {
                            net = Some(n);

                            if let Ok(classes_path) = std::env::var("RAYOS_DNN_CLASSES") {
                                if let Ok(file) = std::fs::read_to_string(&classes_path) {
                                    let loaded: Vec<String> = file
                                        .lines()
                                        .map(|l| l.trim())
                                        .filter(|l| !l.is_empty())
                                        .map(|l| l.to_string())
                                        .collect();
                                    if !loaded.is_empty() {
                                        log::info!("Loaded {} class labels from {}", loaded.len(), classes_path);
                                        return Ok(Self {
                                            net,
                                            ssd_input_size,
                                            classes: loaded,
                                        });
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            log::warn!("Failed to load DNN model; falling back to simulated detection: {e}");
                        }
                    }
                } else {
                    log::warn!("DNN paths provided but not found; falling back to simulated detection");
                }
            }

            Ok(Self {
                net,
                ssd_input_size,
                classes,
            })
        }

        #[cfg(not(feature = "vision"))]
        {
            Ok(Self { classes })
        }
    }

    #[cfg(feature = "vision")]
    /// Recognize objects in a frame
    pub fn recognize(&self, frame: &core::Mat) -> Result<Vec<DetectedObject>> {
        if let Some(net) = &self.net {
            return self.recognize_ssd_caffe(net, frame);
        }

        // Fallback: deterministic simulated detections.

        let mut detections = Vec::new();

        // Get frame dimensions
        let rows = frame.rows();
        let cols = frame.cols();

        if rows <= 0 || cols <= 0 {
            return Ok(detections);
        }

        // Simulate detection based on image properties
        // In a real implementation, this would be neural network inference

        // Generate deterministic "detections" based on frame hash
        let frame_hash = self.hash_frame(frame)?;

        // Use hash to generate simulated detections
        let num_objects = ((frame_hash % 5) + 1) as usize;

        for i in 0..num_objects {
            let class_idx = ((frame_hash + i as u64) % self.classes.len() as u64) as usize;
            let x = ((frame_hash * (i as u64 + 1)) % 70) as f32 / 100.0;
            let y = ((frame_hash * (i as u64 + 2)) % 70) as f32 / 100.0;
            let w = 0.15 + ((frame_hash * (i as u64 + 3)) % 15) as f32 / 100.0;
            let h = 0.15 + ((frame_hash * (i as u64 + 4)) % 15) as f32 / 100.0;
            let conf = 0.5 + ((frame_hash * (i as u64 + 5)) % 40) as f32 / 100.0;

            detections.push(DetectedObject {
                label: self.classes[class_idx].clone(),
                confidence: conf,
                bbox: BoundingBox { x, y, width: w, height: h },
            });
        }

        log::debug!("Simulated detection: found {} objects", detections.len());
        Ok(detections)
    }

    #[cfg(feature = "vision")]
    /// Hash a frame to generate deterministic but varied results
    fn hash_frame(&self, frame: &core::Mat) -> Result<u64> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash dimensions and type
        frame.rows().hash(&mut hasher);
        frame.cols().hash(&mut hasher);
        frame.typ().hash(&mut hasher);

        // Sample a few pixels
        for i in (0..frame.rows()).step_by(frame.rows().max(1) as usize / 10) {
            for j in (0..frame.cols()).step_by(frame.cols().max(1) as usize / 10) {
                // Hash pixel location (actual pixel access would require unsafe)
                (i, j).hash(&mut hasher);
            }
        }

        Ok(hasher.finish())
    }

    #[cfg(feature = "vision")]
    fn recognize_ssd_caffe(&self, net: &dnn::Net, frame: &core::Mat) -> Result<Vec<DetectedObject>> {
        // MobileNet-SSD style detection output parsing.
        // Output rows: N detections, each is 7 floats:
        // [image_id, class_id, confidence, x1, y1, x2, y2]

        let mut net = net.clone();

        let blob = dnn::blob_from_image(
            frame,
            0.007843, // 1/127.5
            self.ssd_input_size,
            core::Scalar::new(127.5, 127.5, 127.5, 0.0),
            false,
            false,
            core::CV_32F,
        )
        .context("blob_from_image")?;

        net.set_input(&blob, "", 1.0, core::Scalar::default())
            .context("net.set_input")?;

        let mut out = net.forward("").context("net.forward")?;

        // Flatten to f32 slice.
        let data = unsafe { out.data_typed::<f32>() }.context("out.data_typed")?;
        if data.len() < 7 {
            return Ok(vec![]);
        }

        let mut detections = Vec::new();
        for det in data.chunks_exact(7) {
            let class_id = det[1] as i32;
            let conf = det[2];
            if conf < 0.5 {
                continue;
            }

            let x1 = det[3].clamp(0.0, 1.0);
            let y1 = det[4].clamp(0.0, 1.0);
            let x2 = det[5].clamp(0.0, 1.0);
            let y2 = det[6].clamp(0.0, 1.0);

            let (x, y, w, h) = (x1, y1, (x2 - x1).max(0.0), (y2 - y1).max(0.0));

            let label = self
                .classes
                .get(class_id.saturating_sub(1) as usize)
                .cloned()
                .unwrap_or_else(|| format!("class_{class_id}"));

            detections.push(DetectedObject {
                label,
                confidence: conf.clamp(0.0, 1.0),
                bbox: BoundingBox {
                    x,
                    y,
                    width: w,
                    height: h,
                },
            });
        }

        Ok(detections)
    }

    /// Check if a specific object class is present
    pub fn is_holding_coffee(&self, detections: &[DetectedObject]) -> bool {
        detections.iter().any(|obj| {
            (obj.label == "cup" || obj.label == "bottle") && obj.confidence > 0.6
        })
    }

    /// Get the dominant object in view
    pub fn get_dominant_object<'a>(&self, detections: &'a [DetectedObject]) -> Option<&'a DetectedObject> {
        detections.iter()
            .max_by(|a, b| {
                let area_a = a.bbox.width * a.bbox.height;
                let area_b = b.bbox.width * b.bbox.height;
                area_a.partial_cmp(&area_b).unwrap()
            })
    }
}
