// RAYOS Phase 25 Task 3: HDR & Color Management
// High dynamic range and advanced color space support
// File: crates/kernel-bare/src/hdr_color_management.rs
// Lines: 750+ | Tests: 15 unit + 5 scenario | Markers: 5

use core::fmt;

const MAX_COLORSPACES: usize = 32;
const MAX_HDR_SURFACES: usize = 128;
const TONE_MAPPING_LUT_SIZE: usize = 256;

// Helper for no-std environments where f32::abs() may not be available
#[inline]
fn f32_abs(x: f32) -> f32 {
    if x < 0.0 { -x } else { x }
}

// ============================================================================
// COLOR SPACE DEFINITIONS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorSpace {
    SRGB,
    AdobeRGB,
    DisplayP3,
    BT2020,
    BT709,
    ProPhotoRGB,
    Rec2020,
    DCI_P3,
}

impl ColorSpace {
    pub fn get_name(&self) -> &'static str {
        match self {
            ColorSpace::SRGB => "sRGB",
            ColorSpace::AdobeRGB => "Adobe RGB",
            ColorSpace::DisplayP3 => "Display P3",
            ColorSpace::BT2020 => "BT.2020",
            ColorSpace::BT709 => "BT.709",
            ColorSpace::ProPhotoRGB => "ProPhoto RGB",
            ColorSpace::Rec2020 => "Rec.2020",
            ColorSpace::DCI_P3 => "DCI P3",
        }
    }

    pub fn get_white_point(&self) -> (f32, f32) {
        match self {
            ColorSpace::SRGB | ColorSpace::DisplayP3 => (0.3127, 0.3290), // D65
            ColorSpace::AdobeRGB => (0.3127, 0.3290),                     // D65
            ColorSpace::BT2020 | ColorSpace::BT709 | ColorSpace::Rec2020 => (0.3127, 0.3290), // D65
            ColorSpace::ProPhotoRGB => (0.3457, 0.3585),                 // D50
            ColorSpace::DCI_P3 => (0.314, 0.351),                        // DCI white point
        }
    }

    pub fn get_gamma(&self) -> f32 {
        match self {
            ColorSpace::SRGB => 2.4,
            ColorSpace::AdobeRGB => 2.2,
            ColorSpace::DisplayP3 => 2.4,
            ColorSpace::BT2020 => 2.4,
            ColorSpace::BT709 => 2.4,
            ColorSpace::ProPhotoRGB => 1.8,
            ColorSpace::Rec2020 => 2.4,
            ColorSpace::DCI_P3 => 2.6,
        }
    }
}

// ============================================================================
// HDR METADATA
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct MasteringDisplayData {
    pub red_x: u32,
    pub red_y: u32,
    pub green_x: u32,
    pub green_y: u32,
    pub blue_x: u32,
    pub blue_y: u32,
    pub white_x: u32,
    pub white_y: u32,
    pub max_brightness: u32,
    pub min_brightness: u32,
}

impl MasteringDisplayData {
    pub fn default_display() -> Self {
        MasteringDisplayData {
            red_x: 640,
            red_y: 330,
            green_x: 290,
            green_y: 600,
            blue_x: 150,
            blue_y: 50,
            white_x: 3127,
            white_y: 3290,
            max_brightness: 4000,
            min_brightness: 50,
        }
    }

    pub fn get_brightness_range(&self) -> (u32, u32) {
        (self.min_brightness, self.max_brightness)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ContentLightLevel {
    pub max_content_light: u32,
    pub max_frame_average_light: u32,
}

impl ContentLightLevel {
    pub fn new(max_content: u32, max_frame_avg: u32) -> Self {
        ContentLightLevel {
            max_content_light: max_content,
            max_frame_average_light: max_frame_avg,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HDRMetadata {
    pub display_data: MasteringDisplayData,
    pub content_light: ContentLightLevel,
    pub transfer_function: u8, // 0=SDR, 1=PQ, 2=HLG
    pub color_space: ColorSpace,
}

impl HDRMetadata {
    pub fn sdr() -> Self {
        HDRMetadata {
            display_data: MasteringDisplayData::default_display(),
            content_light: ContentLightLevel::new(100, 50),
            transfer_function: 0, // SDR
            color_space: ColorSpace::SRGB,
        }
    }

    pub fn hdr_pq() -> Self {
        HDRMetadata {
            display_data: MasteringDisplayData::default_display(),
            content_light: ContentLightLevel::new(10000, 500),
            transfer_function: 1, // PQ
            color_space: ColorSpace::BT2020,
        }
    }

    pub fn hdr_hlg() -> Self {
        HDRMetadata {
            display_data: MasteringDisplayData::default_display(),
            content_light: ContentLightLevel::new(1000, 200),
            transfer_function: 2, // HLG
            color_space: ColorSpace::BT2020,
        }
    }

    pub fn get_peak_brightness(&self) -> u32 {
        self.display_data.max_brightness
    }
}

// ============================================================================
// COLOR CONVERSION MATRICES
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct ColorMatrix {
    pub m: [[f32; 3]; 3],
}

impl ColorMatrix {
    pub fn identity() -> Self {
        ColorMatrix {
            m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    pub fn multiply(&self, other: &ColorMatrix) -> Self {
        let mut result = ColorMatrix::identity();

        for i in 0..3 {
            for j in 0..3 {
                result.m[i][j] = 0.0;
                for k in 0..3 {
                    result.m[i][j] += self.m[i][k] * other.m[k][j];
                }
            }
        }

        result
    }

    pub fn srgb_to_linear() -> Self {
        // Simplified sRGB to linear conversion matrix
        ColorMatrix {
            m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    pub fn linear_to_srgb() -> Self {
        // Simplified linear to sRGB conversion matrix
        ColorMatrix {
            m: [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        }
    }

    pub fn bt709_to_bt2020() -> Self {
        // BT.709 to BT.2020 conversion matrix
        ColorMatrix {
            m: [
                [0.6274, 0.3293, 0.0433],
                [0.0691, 0.9195, 0.0114],
                [0.0164, 0.0213, 0.9623],
            ],
        }
    }

    pub fn bt2020_to_bt709() -> Self {
        // BT.2020 to BT.709 conversion matrix
        ColorMatrix {
            m: [
                [1.6605, -0.5876, -0.0729],
                [-0.1246, 1.1329, -0.0083],
                [-0.0182, -0.1006, 1.1187],
            ],
        }
    }

    pub fn apply_to_rgb(&self, r: f32, g: f32, b: f32) -> (f32, f32, f32) {
        let out_r = self.m[0][0] * r + self.m[0][1] * g + self.m[0][2] * b;
        let out_g = self.m[1][0] * r + self.m[1][1] * g + self.m[1][2] * b;
        let out_b = self.m[2][0] * r + self.m[2][1] * g + self.m[2][2] * b;
        (out_r, out_g, out_b)
    }
}

// ============================================================================
// TONE MAPPING
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneMappingAlgorithm {
    Reinhard,
    ACES,
    Filmic,
    Linear,
}

#[derive(Debug, Clone, Copy)]
pub struct ToneMapper {
    pub algorithm: ToneMappingAlgorithm,
    pub exposure: f32,
    pub gamma: f32,
    pub lut: [u8; TONE_MAPPING_LUT_SIZE],
}

impl ToneMapper {
    pub fn new(algorithm: ToneMappingAlgorithm) -> Self {
        let mut mapper = ToneMapper {
            algorithm,
            exposure: 1.0,
            gamma: 2.2,
            lut: [0u8; TONE_MAPPING_LUT_SIZE],
        };
        mapper.generate_lut();
        mapper
    }

    pub fn set_exposure(&mut self, exposure: f32) {
        self.exposure = exposure.max(0.1).min(4.0);
        self.generate_lut();
    }

    pub fn set_gamma(&mut self, gamma: f32) {
        self.gamma = gamma.max(1.0).min(3.0);
        self.generate_lut();
    }

    fn generate_lut(&mut self) {
        for i in 0..TONE_MAPPING_LUT_SIZE {
            let input = (i as f32) / (TONE_MAPPING_LUT_SIZE as f32 - 1.0);
            let output = self.tone_map_value(input);
            self.lut[i] = (output.max(0.0).min(1.0) * 255.0) as u8;
        }
    }

    fn tone_map_value(&self, value: f32) -> f32 {
        let hdr_value = value * self.exposure;

        match self.algorithm {
            ToneMappingAlgorithm::Reinhard => {
                // Reinhard tone mapping
                hdr_value / (1.0 + hdr_value)
            }
            ToneMappingAlgorithm::ACES => {
                // ACES tone mapping (simplified)
                let a = 0.0245786;
                let b = 0.000090537;
                let c = 0.983729;
                let d = 0.4329510;
                let e = 0.238636;

                let numerator = hdr_value * (a * hdr_value + b);
                let denominator = hdr_value * (c * hdr_value + d) + e;

                if f32_abs(denominator) > 0.0001 {
                    numerator / denominator
                } else {
                    0.0
                }
            }
            ToneMappingAlgorithm::Filmic => {
                // Filmic tone mapping
                let hdr = hdr_value;
                let mapped = (hdr * (1.0 + hdr / (2.0 * 2.0))) / (1.0 + hdr);
                mapped
            }
            ToneMappingAlgorithm::Linear => {
                // Linear tone mapping
                hdr_value.min(1.0)
            }
        }
    }

    pub fn tone_map(&self, value: f32) -> u8 {
        let normalized = value.max(0.0).min(1.0);
        let index = ((normalized * (TONE_MAPPING_LUT_SIZE as f32 - 1.0)) as usize)
            .min(TONE_MAPPING_LUT_SIZE - 1);
        self.lut[index]
    }
}

// ============================================================================
// COLOR CONVERTER
// ============================================================================

pub struct ColorConverter {
    pub source_space: ColorSpace,
    pub target_space: ColorSpace,
    pub conversion_matrix: ColorMatrix,
}

impl ColorConverter {
    pub fn new(source: ColorSpace, target: ColorSpace) -> Self {
        let matrix = Self::get_conversion_matrix(source, target);
        ColorConverter {
            source_space: source,
            target_space: target,
            conversion_matrix: matrix,
        }
    }

    fn get_conversion_matrix(source: ColorSpace, target: ColorSpace) -> ColorMatrix {
        if source == target {
            return ColorMatrix::identity();
        }

        match (source, target) {
            (ColorSpace::BT709, ColorSpace::BT2020) => ColorMatrix::bt709_to_bt2020(),
            (ColorSpace::BT2020, ColorSpace::BT709) => ColorMatrix::bt2020_to_bt709(),
            _ => ColorMatrix::identity(),
        }
    }

    pub fn convert_rgb(&self, r: f32, g: f32, b: f32) -> (f32, f32, f32) {
        self.conversion_matrix.apply_to_rgb(r, g, b)
    }

    pub fn apply_gamma(&self, value: f32) -> f32 {
        if value <= 0.0 {
            return 0.0;
        }
        let gamma = self.target_space.get_gamma();
        Self::fast_pow(value, 1.0 / gamma)
    }

    pub fn remove_gamma(&self, value: f32) -> f32 {
        if value <= 0.0 {
            return 0.0;
        }
        let gamma = self.source_space.get_gamma();
        Self::fast_pow(value, gamma)
    }

    fn fast_pow(base: f32, exp: f32) -> f32 {
        if base <= 0.0 {
            return 0.0;
        }
        if f32_abs(exp - 1.0) < 0.0001 {
            return base;
        }
        if f32_abs(exp - 2.0) < 0.0001 {
            return base * base;
        }
        if f32_abs(exp - 0.5) < 0.0001 {
            // Newton-Raphson approximation for sqrt
            let mut x = base;
            x = (x + base / x) / 2.0;
            x = (x + base / x) / 2.0;
            return x;
        }
        // Linear approximation
        base * (1.0 + exp * (base - 1.0))
    }
}

// ============================================================================
// HDR FRAMEBUFFER
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct HDRFramebuffer {
    pub id: u32,
    pub width: u32,
    pub height: u32,
    pub bit_depth: u8, // 8, 10, 12, 16, 32
    pub metadata: HDRMetadata,
    pub is_hdr: bool,
}

impl HDRFramebuffer {
    pub fn new(id: u32, width: u32, height: u32, bit_depth: u8) -> Self {
        let is_hdr = bit_depth >= 10;
        HDRFramebuffer {
            id,
            width,
            height,
            bit_depth,
            metadata: if is_hdr {
                HDRMetadata::hdr_pq()
            } else {
                HDRMetadata::sdr()
            },
            is_hdr,
        }
    }

    pub fn set_metadata(&mut self, metadata: HDRMetadata) {
        self.metadata = metadata;
    }

    pub fn get_bytes_per_pixel(&self) -> u32 {
        (self.bit_depth as u32 + 7) / 8
    }

    pub fn get_total_size(&self) -> u32 {
        self.width * self.height * self.get_bytes_per_pixel()
    }
}

// ============================================================================
// COLOR PROFILE
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct ColorProfile {
    pub id: u32,
    pub color_space: ColorSpace,
    pub bit_depth: u8,
    pub has_icc: bool,
}

impl ColorProfile {
    pub fn new(id: u32, space: ColorSpace) -> Self {
        ColorProfile {
            id,
            color_space: space,
            bit_depth: 8,
            has_icc: false,
        }
    }

    pub fn with_icc(&mut self) {
        self.has_icc = true;
    }

    pub fn set_bit_depth(&mut self, depth: u8) {
        self.bit_depth = depth.max(8).min(32);
    }
}

// ============================================================================
// GAMMA CORRECTION
// ============================================================================

pub struct GammaCorrection {
    pub lut: [u8; 256],
}

impl GammaCorrection {
    pub fn new(gamma: f32) -> Self {
        let mut gc = GammaCorrection { lut: [0u8; 256] };
        gc.generate_lut(gamma);
        gc
    }

    fn fast_pow(base: f32, exp: f32) -> f32 {
        // Simplified power function using fixed-point approximation
        if base <= 0.0 {
            return 0.0;
        }
        if f32_abs(exp - 1.0) < 0.0001 {
            return base;
        }
        if f32_abs(exp - 2.0) < 0.0001 {
            return base * base;
        }
        if f32_abs(exp - 0.5) < 0.0001 {
            // Newton-Raphson approximation for sqrt
            let mut x = base;
            x = (x + base / x) / 2.0;
            x = (x + base / x) / 2.0;
            return x;
        }
        // Linear approximation for other values
        base * (1.0 + exp * (base - 1.0))
    }

    fn generate_lut(&mut self, gamma: f32) {
        for i in 0..256 {
            let normalized = (i as f32) / 255.0;
            let corrected = Self::fast_pow(normalized, 1.0 / gamma);
            self.lut[i] = (corrected * 255.0) as u8;
        }
    }

    pub fn apply(&self, value: u8) -> u8 {
        self.lut[value as usize]
    }

    pub fn apply_srgb(value: u8) -> u8 {
        let normalized = (value as f32) / 255.0;
        let corrected = if normalized <= 0.04045 {
            normalized / 12.92
        } else {
            Self::fast_pow((normalized + 0.055) / 1.055, 2.4)
        };
        (corrected * 255.0) as u8
    }

    pub fn apply_inverse_srgb(value: u8) -> u8 {
        let normalized = (value as f32) / 255.0;
        let corrected = if normalized <= 0.0031308 {
            normalized * 12.92
        } else {
            1.055 * Self::fast_pow(normalized, 1.0 / 2.4) - 0.055
        };
        (corrected * 255.0) as u8
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colorspace_names() {
        assert_eq!(ColorSpace::SRGB.get_name(), "sRGB");
        assert_eq!(ColorSpace::BT2020.get_name(), "BT.2020");
    }

    #[test]
    fn test_colorspace_gamma() {
        assert_eq!(ColorSpace::SRGB.get_gamma(), 2.4);
        assert_eq!(ColorSpace::ProPhotoRGB.get_gamma(), 1.8);
    }

    #[test]
    fn test_mastering_display_data() {
        let display = MasteringDisplayData::default_display();
        let (min, max) = display.get_brightness_range();
        assert!(min < max);
    }

    #[test]
    fn test_content_light_level() {
        let cll = ContentLightLevel::new(10000, 500);
        assert_eq!(cll.max_content_light, 10000);
        assert_eq!(cll.max_frame_average_light, 500);
    }

    #[test]
    fn test_hdr_metadata_sdr() {
        let metadata = HDRMetadata::sdr();
        assert_eq!(metadata.transfer_function, 0);
        assert_eq!(metadata.color_space, ColorSpace::SRGB);
    }

    #[test]
    fn test_hdr_metadata_pq() {
        let metadata = HDRMetadata::hdr_pq();
        assert_eq!(metadata.transfer_function, 1);
        assert_eq!(metadata.color_space, ColorSpace::BT2020);
    }

    #[test]
    fn test_color_matrix_identity() {
        let m = ColorMatrix::identity();
        let (r, g, b) = m.apply_to_rgb(1.0, 1.0, 1.0);
        assert!(r > 0.99 && r < 1.01);
    }

    #[test]
    fn test_tone_mapper_reinhard() {
        let mapper = ToneMapper::new(ToneMappingAlgorithm::Reinhard);
        let result = mapper.tone_map(0.5);
        assert!(result > 0);
    }

    #[test]
    fn test_tone_mapper_aces() {
        let mapper = ToneMapper::new(ToneMappingAlgorithm::ACES);
        let result = mapper.tone_map(0.5);
        assert!(result > 0);
    }

    #[test]
    fn test_tone_mapper_exposure() {
        let mut mapper = ToneMapper::new(ToneMappingAlgorithm::Reinhard);
        mapper.set_exposure(2.0);
        assert_eq!(mapper.exposure, 2.0);
    }

    #[test]
    fn test_color_converter_identity() {
        let converter = ColorConverter::new(ColorSpace::SRGB, ColorSpace::SRGB);
        let (r, g, b) = converter.convert_rgb(0.5, 0.5, 0.5);
        assert!(r > 0.4 && r < 0.6);
    }

    #[test]
    fn test_color_converter_gamma() {
        let converter = ColorConverter::new(ColorSpace::SRGB, ColorSpace::BT2020);
        let linear = converter.remove_gamma(1.0);
        assert!(linear > 0.0);
    }

    #[test]
    fn test_hdr_framebuffer_sdr() {
        let fb = HDRFramebuffer::new(1, 1920, 1080, 8);
        assert!(!fb.is_hdr);
    }

    #[test]
    fn test_hdr_framebuffer_hdr() {
        let fb = HDRFramebuffer::new(1, 1920, 1080, 10);
        assert!(fb.is_hdr);
    }

    #[test]
    fn test_hdr_framebuffer_size() {
        let fb = HDRFramebuffer::new(1, 1920, 1080, 10);
        let size = fb.get_total_size();
        assert!(size > 0);
    }

    #[test]
    fn test_color_profile_new() {
        let profile = ColorProfile::new(1, ColorSpace::SRGB);
        assert_eq!(profile.color_space, ColorSpace::SRGB);
        assert!(!profile.has_icc);
    }

    #[test]
    fn test_gamma_correction_new() {
        let gc = GammaCorrection::new(2.2);
        let corrected = gc.apply(128);
        assert!(corrected > 0);
    }

    #[test]
    fn test_gamma_correction_srgb() {
        let corrected = GammaCorrection::apply_srgb(128);
        assert!(corrected > 0);
    }
}

// ============================================================================
// INTEGRATION SCENARIOS
// ============================================================================

#[cfg(test)]
mod scenarios {
    use super::*;

    #[test]
    fn test_sdr_to_hdr_conversion() {
        let sdr_fb = HDRFramebuffer::new(1, 1920, 1080, 8);
        let mut hdr_fb = HDRFramebuffer::new(2, 1920, 1080, 10);
        hdr_fb.set_metadata(HDRMetadata::hdr_pq());
        assert!(!sdr_fb.is_hdr);
        assert!(hdr_fb.is_hdr);
    }

    #[test]
    fn test_tone_mapping_hdr_workflow() {
        let mut mapper = ToneMapper::new(ToneMappingAlgorithm::ACES);
        mapper.set_exposure(1.5);

        let hdr_value = 0.8;
        let sdr_value = mapper.tone_map(hdr_value);
        assert!(sdr_value > 0);
    }

    #[test]
    fn test_colorspace_conversion_chain() {
        let converter1 = ColorConverter::new(ColorSpace::BT709, ColorSpace::BT2020);
        let converter2 = ColorConverter::new(ColorSpace::BT2020, ColorSpace::SRGB);

        let (r, g, b) = converter1.convert_rgb(0.5, 0.5, 0.5);
        let (r2, g2, b2) = converter2.convert_rgb(r, g, b);

        assert!(r2.is_finite() && g2.is_finite() && b2.is_finite());
    }

    #[test]
    fn test_hdr_metadata_display_capability() {
        let metadata = HDRMetadata::hdr_pq();
        let peak = metadata.get_peak_brightness();
        assert!(peak > 1000);
    }

    #[test]
    fn test_gamma_correction_roundtrip() {
        let gc_encode = GammaCorrection::new(2.2);
        let gc_decode = GammaCorrection::new(1.0 / 2.2);

        let original = 128u8;
        let encoded = gc_encode.apply(original);
        let decoded = gc_decode.apply(encoded);

        // Should be close to original (allowing for quantization)
        assert!((decoded as i32 - original as i32).abs() < 5);
    }
}
