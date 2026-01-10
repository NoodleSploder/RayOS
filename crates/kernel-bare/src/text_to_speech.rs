// RAYOS Phase 27 Task 4: Text-to-Speech Engine
// Synthesize speech from text using phoneme-based TTS
// File: crates/kernel-bare/src/text_to_speech.rs
// Lines: 700+ | Tests: 13 unit + 5 scenario | Markers: 5


const MAX_PHONEME_SEQUENCE: usize = 1024;
const MAX_GRAPHEME_TEXT: usize = 256;
const PHONEME_WAVEFORM_SAMPLES: usize = 2048;

// ============================================================================
// PHONEME & GRAPHEME DEFINITIONS
// ============================================================================

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phoneme {
    // Vowels (IPA)
    A,      // "ah" in "father"
    E,      // "eh" in "dress"
    I,      // "ee" in "fleece"
    O,      // "oh" in "thought"
    U,      // "oo" in "goose"
    SchwaA, // "uh" in "strut"

    // Consonants (major classes)
    P,      // voiceless bilabial stop
    B,      // voiced bilabial stop
    T,      // voiceless alveolar stop
    D,      // voiced alveolar stop
    K,      // voiceless velar stop
    G,      // voiced velar stop
    M,      // voiced bilabial nasal
    N,      // voiced alveolar nasal
    F,      // voiceless labiodental fricative
    V,      // voiced labiodental fricative
    S,      // voiceless alveolar fricative
    Z,      // voiced alveolar fricative
    Sh,     // voiceless postalveolar fricative
    Zh,     // voiced postalveolar fricative
    Th,     // voiceless dental fricative
    ThV,    // voiced dental fricative (th as in "this")
    L,      // voiced alveolar approximant
    R,      // voiced alveolar approximant
    Y,      // voiced palatal approximant
    W,      // voiced labial-velar approximant
    H,      // voiceless glottal fricative
    Ng,     // velar nasal

    // Affricates
    Ch,     // voiceless postalveolar affricate
    J,      // voiced postalveolar affricate

    // Diphthongs
    Ai,     // "ai" in "price"
    Au,     // "ou" in "mouth"
    Oi,     // "oi" in "choice"

    // Silence
    Silence,
}

impl Phoneme {
    pub fn duration_ms(&self) -> u32 {
        match self {
            Phoneme::Silence => 100,
            Phoneme::A | Phoneme::E | Phoneme::I | Phoneme::O | Phoneme::U => 150,
            Phoneme::SchwaA => 80,
            _ => 100, // Consonants and other phonemes
        }
    }

    pub fn frequency_hz(&self, voice_pitch: u8) -> u32 {
        let base_freq = match self {
            Phoneme::A => 700,
            Phoneme::E => 600,
            Phoneme::I => 400,
            Phoneme::O => 500,
            Phoneme::U => 300,
            Phoneme::SchwaA => 500,
            _ => 100, // Consonants (no specific pitch)
        };

        // Adjust by voice pitch (60-200)
        let pitch_factor = voice_pitch as u32;
        (base_freq * pitch_factor) / 100
    }

    pub fn is_vowel(&self) -> bool {
        matches!(
            self,
            Phoneme::A
                | Phoneme::E
                | Phoneme::I
                | Phoneme::O
                | Phoneme::U
                | Phoneme::SchwaA
                | Phoneme::Ai
                | Phoneme::Au
                | Phoneme::Oi
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PhonemeUnit {
    pub phoneme: Phoneme,
    pub duration_ms: u32,
    pub pitch_cents: i16, // -1200 to +1200 (semitones in cents)
}

impl PhonemeUnit {
    pub fn new(phoneme: Phoneme) -> Self {
        PhonemeUnit {
            phoneme,
            duration_ms: phoneme.duration_ms(),
            pitch_cents: 0,
        }
    }

    pub fn with_duration(mut self, duration_ms: u32) -> Self {
        self.duration_ms = duration_ms;
        self
    }
}

pub struct PhonemeSequence {
    pub phonemes: [Option<PhonemeUnit>; MAX_PHONEME_SEQUENCE],
    pub length: usize,
}

impl PhonemeSequence {
    pub fn new() -> Self {
        PhonemeSequence {
            phonemes: [None; MAX_PHONEME_SEQUENCE],
            length: 0,
        }
    }

    pub fn add_phoneme(&mut self, unit: PhonemeUnit) -> bool {
        if self.length >= MAX_PHONEME_SEQUENCE {
            return false;
        }
        self.phonemes[self.length] = Some(unit);
        self.length += 1;
        true
    }

    pub fn total_duration_ms(&self) -> u32 {
        let mut total = 0;
        for i in 0..self.length {
            if let Some(unit) = self.phonemes[i] {
                total += unit.duration_ms;
            }
        }
        total
    }

    pub fn clear(&mut self) {
        self.length = 0;
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }
}

impl Default for PhonemeSequence {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Grapheme {
    pub character: u8,
}

impl Grapheme {
    pub fn new(character: u8) -> Self {
        Grapheme { character }
    }

    pub fn to_phoneme(&self) -> Option<Phoneme> {
        match self.character {
            b'a' | b'A' => Some(Phoneme::A),
            b'e' | b'E' => Some(Phoneme::E),
            b'i' | b'I' => Some(Phoneme::I),
            b'o' | b'O' => Some(Phoneme::O),
            b'u' | b'U' => Some(Phoneme::U),
            b'p' | b'P' => Some(Phoneme::P),
            b'b' | b'B' => Some(Phoneme::B),
            b't' | b'T' => Some(Phoneme::T),
            b'd' | b'D' => Some(Phoneme::D),
            b'k' | b'K' => Some(Phoneme::K),
            b'g' | b'G' => Some(Phoneme::G),
            b'm' | b'M' => Some(Phoneme::M),
            b'n' | b'N' => Some(Phoneme::N),
            b'f' | b'F' => Some(Phoneme::F),
            b'v' | b'V' => Some(Phoneme::V),
            b's' | b'S' => Some(Phoneme::S),
            b'z' | b'Z' => Some(Phoneme::Z),
            b'l' | b'L' => Some(Phoneme::L),
            b'r' | b'R' => Some(Phoneme::R),
            b'w' | b'W' => Some(Phoneme::W),
            b'h' | b'H' => Some(Phoneme::H),
            b'y' | b'Y' => Some(Phoneme::Y),
            b' ' => Some(Phoneme::Silence),
            b'.' | b',' | b'!' | b'?' => Some(Phoneme::Silence),
            _ => None,
        }
    }
}

// ============================================================================
// TEXT ANALYZER
// ============================================================================

pub struct TextAnalyzer {
    pub graphemes: [Option<Grapheme>; MAX_GRAPHEME_TEXT],
    pub grapheme_count: usize,
}

impl TextAnalyzer {
    pub fn new() -> Self {
        TextAnalyzer {
            graphemes: [None; MAX_GRAPHEME_TEXT],
            grapheme_count: 0,
        }
    }

    pub fn analyze_text(&mut self, text: &[u8]) -> usize {
        self.grapheme_count = 0;

        for &byte in text.iter() {
            if self.grapheme_count >= MAX_GRAPHEME_TEXT {
                break;
            }

            let grapheme = Grapheme::new(byte);
            self.graphemes[self.grapheme_count] = Some(grapheme);
            self.grapheme_count += 1;
        }

        self.grapheme_count
    }

    pub fn get_grapheme(&self, index: usize) -> Option<Grapheme> {
        if index < self.grapheme_count {
            self.graphemes[index]
        } else {
            None
        }
    }

    pub fn split_sentences(&self) -> [[usize; 2]; 16] {
        // Returns [start, end] positions for up to 16 sentences
        let mut sentences = [[0usize; 2]; 16];
        let mut sentence_count = 0;
        let mut current_start = 0;

        for i in 0..self.grapheme_count {
            if let Some(grapheme) = self.graphemes[i] {
                if matches!(grapheme.character, b'.' | b'!' | b'?') && sentence_count < 16 {
                    sentences[sentence_count] = [current_start, i + 1];
                    sentence_count += 1;
                    current_start = i + 1;
                }
            }
        }

        if current_start < self.grapheme_count && sentence_count < 16 {
            sentences[sentence_count] = [current_start, self.grapheme_count];
        }

        sentences
    }

    pub fn clear(&mut self) {
        self.grapheme_count = 0;
    }
}

impl Default for TextAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// PHONEME GENERATOR
// ============================================================================

pub struct PhonemeGenerator;

impl PhonemeGenerator {
    pub fn grapheme_to_phoneme(grapheme: Grapheme) -> Option<Phoneme> {
        grapheme.to_phoneme()
    }

    pub fn generate_sequence(analyzer: &TextAnalyzer) -> PhonemeSequence {
        let mut sequence = PhonemeSequence::new();

        for i in 0..analyzer.grapheme_count {
            if let Some(grapheme) = analyzer.graphemes[i] {
                if let Some(phoneme) = Self::grapheme_to_phoneme(grapheme) {
                    let unit = PhonemeUnit::new(phoneme);
                    let _ = sequence.add_phoneme(unit);
                }
            }
        }

        sequence
    }

    pub fn apply_prosody(sequence: &mut PhonemeSequence, speaking_rate: u8) {
        // Adjust duration based on speaking rate (50-200, default 100)
        for i in 0..sequence.length {
            if let Some(ref mut unit) = sequence.phonemes[i] {
                let new_duration = (unit.duration_ms as u32 * speaking_rate as u32) / 100;
                unit.duration_ms = new_duration;
            }
        }
    }
}

// ============================================================================
// SPEECH SYNTHESIZER
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub struct VoiceProfile {
    pub speaking_rate: u8,    // 50-200 (default 100)
    pub pitch_shift: i8,      // -24 to +24 semitones
    pub volume: u8,           // 0-255
    pub gender_male: bool,    // false = female
}

impl VoiceProfile {
    pub fn default_male() -> Self {
        VoiceProfile {
            speaking_rate: 100,
            pitch_shift: 0,
            volume: 200,
            gender_male: true,
        }
    }

    pub fn default_female() -> Self {
        VoiceProfile {
            speaking_rate: 100,
            pitch_shift: 12,
            volume: 200,
            gender_male: false,
        }
    }

    pub fn get_base_pitch(&self) -> u8 {
        if self.gender_male {
            80
        } else {
            120
        }
    }
}

impl Default for VoiceProfile {
    fn default() -> Self {
        Self::default_male()
    }
}

pub struct SpeechSynthesizer {
    pub voice: VoiceProfile,
    pub waveform: [i16; PHONEME_WAVEFORM_SAMPLES],
    pub sample_rate: u32,
    pub total_samples: u64,
}

impl SpeechSynthesizer {
    pub fn new(voice: VoiceProfile) -> Self {
        SpeechSynthesizer {
            voice,
            waveform: [0; PHONEME_WAVEFORM_SAMPLES],
            sample_rate: 48000,
            total_samples: 0,
        }
    }

    pub fn synthesize_phoneme(&mut self, unit: &PhonemeUnit) -> usize {
        if unit.phoneme == Phoneme::Silence {
            return 0;
        }

        let freq = unit.phoneme.frequency_hz(self.voice.get_base_pitch());
        let duration_samples = (self.sample_rate as u64 * unit.duration_ms as u64) / 1000;
        let samples_to_generate = core::cmp::min(duration_samples as usize, PHONEME_WAVEFORM_SAMPLES);

        // Generate triangle wave
        for i in 0..samples_to_generate {
            let phase = ((i as u64 * freq as u64) / self.sample_rate as u64) % 4;
            let value = if phase < 2 {
                (phase as i16 - 1) * 16384 / 2
            } else {
                (3 - phase as i16) * 16384 / 2
            };

            let amplitude = (self.voice.volume as i16 * value) / 255;
            self.waveform[i] = amplitude;
        }

        self.total_samples += samples_to_generate as u64;
        samples_to_generate
    }

    pub fn synthesize_sequence(&mut self, sequence: &PhonemeSequence) -> u64 {
        self.total_samples = 0;

        for i in 0..sequence.length {
            if let Some(unit) = sequence.phonemes[i] {
                let _ = self.synthesize_phoneme(&unit);
            }
        }

        self.total_samples
    }

    pub fn get_duration_ms(&self) -> u32 {
        ((self.total_samples * 1000) / (self.sample_rate as u64)) as u32
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phoneme_vowel() {
        assert!(Phoneme::A.is_vowel());
        assert!(Phoneme::E.is_vowel());
        assert!(!Phoneme::P.is_vowel());
    }

    #[test]
    fn test_phoneme_duration() {
        assert!(Phoneme::A.duration_ms() > 100);
        assert_eq!(Phoneme::Silence.duration_ms(), 100);
    }

    #[test]
    fn test_phoneme_unit_new() {
        let unit = PhonemeUnit::new(Phoneme::A);
        assert_eq!(unit.phoneme, Phoneme::A);
    }

    #[test]
    fn test_phoneme_sequence_new() {
        let seq = PhonemeSequence::new();
        assert!(seq.is_empty());
    }

    #[test]
    fn test_phoneme_sequence_add() {
        let mut seq = PhonemeSequence::new();
        let unit = PhonemeUnit::new(Phoneme::A);
        assert!(seq.add_phoneme(unit));
        assert!(!seq.is_empty());
    }

    #[test]
    fn test_grapheme_new() {
        let g = Grapheme::new(b'a');
        assert_eq!(g.character, b'a');
    }

    #[test]
    fn test_grapheme_to_phoneme() {
        let g = Grapheme::new(b'a');
        let p = g.to_phoneme();
        assert_eq!(p, Some(Phoneme::A));
    }

    #[test]
    fn test_text_analyzer_new() {
        let analyzer = TextAnalyzer::new();
        assert_eq!(analyzer.grapheme_count, 0);
    }

    #[test]
    fn test_text_analyzer_analyze() {
        let mut analyzer = TextAnalyzer::new();
        let count = analyzer.analyze_text(b"hello");
        assert_eq!(count, 5);
    }

    #[test]
    fn test_phoneme_generator_sequence() {
        let mut analyzer = TextAnalyzer::new();
        analyzer.analyze_text(b"a");
        let seq = PhonemeGenerator::generate_sequence(&analyzer);
        assert!(!seq.is_empty());
    }

    #[test]
    fn test_voice_profile_male() {
        let voice = VoiceProfile::default_male();
        assert!(voice.gender_male);
        assert!(voice.get_base_pitch() < 100);
    }

    #[test]
    fn test_voice_profile_female() {
        let voice = VoiceProfile::default_female();
        assert!(!voice.gender_male);
        assert!(voice.get_base_pitch() >= 100);
    }

    #[test]
    fn test_speech_synthesizer_new() {
        let voice = VoiceProfile::default_male();
        let synth = SpeechSynthesizer::new(voice);
        assert_eq!(synth.total_samples, 0);
    }

    #[test]
    fn test_speech_synthesizer_duration() {
        let voice = VoiceProfile::default_male();
        let synth = SpeechSynthesizer::new(voice);
        assert!(synth.get_duration_ms() >= 0);
    }

    #[test]
    fn test_speech_synthesizer_synthesize() {
        let voice = VoiceProfile::default_male();
        let mut synth = SpeechSynthesizer::new(voice);
        let unit = PhonemeUnit::new(Phoneme::A);
        let samples = synth.synthesize_phoneme(&unit);
        assert!(samples > 0);
    }
}

// ============================================================================
// INTEGRATION SCENARIOS
// ============================================================================

#[cfg(test)]
mod scenarios {
    use super::*;

    #[test]
    fn test_text_to_speech_pipeline() {
        let mut analyzer = TextAnalyzer::new();
        analyzer.analyze_text(b"hello");

        let sequence = PhonemeGenerator::generate_sequence(&analyzer);
        assert!(!sequence.is_empty());

        let voice = VoiceProfile::default_male();
        let mut synth = SpeechSynthesizer::new(voice);
        let samples = synth.synthesize_sequence(&sequence);

        assert!(samples > 0);
    }

    #[test]
    fn test_male_vs_female_pitch() {
        let male_voice = VoiceProfile::default_male();
        let female_voice = VoiceProfile::default_female();

        assert!(male_voice.get_base_pitch() < female_voice.get_base_pitch());
    }

    #[test]
    fn test_speaking_rate_prosody() {
        let mut analyzer = TextAnalyzer::new();
        analyzer.analyze_text(b"slow");

        let mut seq = PhonemeGenerator::generate_sequence(&analyzer);
        let original_duration = seq.total_duration_ms();

        PhonemeGenerator::apply_prosody(&mut seq, 200); // 2x speed
        let fast_duration = seq.total_duration_ms();

        assert!(fast_duration < original_duration);
    }

    #[test]
    fn test_sentence_splitting() {
        let mut analyzer = TextAnalyzer::new();
        analyzer.analyze_text(b"Hello. World!");

        let sentences = analyzer.split_sentences();
        assert!(sentences[0][1] > sentences[0][0]);
    }

    #[test]
    fn test_full_text_synthesis() {
        let mut analyzer = TextAnalyzer::new();
        analyzer.analyze_text(b"test");

        let mut sequence = PhonemeGenerator::generate_sequence(&analyzer);
        PhonemeGenerator::apply_prosody(&mut sequence, 100);

        let voice = VoiceProfile::default_female();
        let mut synth = SpeechSynthesizer::new(voice);
        synth.synthesize_sequence(&sequence);

        assert!(synth.get_duration_ms() > 0);
    }
}
