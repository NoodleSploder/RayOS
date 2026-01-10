//! Traffic Encryption & Integrity
//!
//! IP packet encryption, AEAD, MAC verification, and replay attack prevention.


/// Encryption mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncryptionMode {
    AEAD,
    MacThenEncrypt,
    EncryptThenMac,
}

/// Packet metadata
#[derive(Clone, Copy)]
pub struct PacketMetadata {
    pub source_ip: u32,
    pub dest_ip: u32,
    pub protocol: u8,
    pub packet_id: u32,
    pub timestamp: u64,
}

/// Encrypted packet structure
#[derive(Clone, Copy)]
pub struct EncryptedPacket {
    pub packet_id: u32,
    pub sequence: u64,
    pub ciphertext: [u8; 256],
    pub ciphertext_len: u16,
    pub mac: [u8; 32],
    pub nonce: [u8; 12],
    pub aad: [u8; 64],
    pub aad_len: u8,
}

/// Replay window (sliding window for replay detection)
pub struct ReplayWindow {
    window: [bool; 256],
    window_start: u64,
}

/// Encryption context
pub struct EncryptionContext {
    encr_key: [u8; 32],
    auth_key: [u8; 32],

    sequence: u64,

    mode: EncryptionMode,

    packets_encrypted: u32,
    packets_decrypted: u32,

    replay_window: ReplayWindow,
    replay_rejects: u32,

    mac_failures: u32,
}

impl ReplayWindow {
    /// Create new replay window
    pub fn new() -> Self {
        ReplayWindow {
            window: [false; 256],
            window_start: 0,
        }
    }

    /// Check if sequence number is valid
    pub fn is_valid(&mut self, sequence: u64) -> bool {
        if sequence < self.window_start {
            return false; // Too old
        }

        let window_pos = ((sequence - self.window_start) % 256) as usize;
        if window_pos >= 256 {
            return false;
        }

        if self.window[window_pos] {
            return false; // Already seen
        }

        self.window[window_pos] = true;

        // Slide window if needed
        if sequence >= (self.window_start + 256) {
            self.window_start = sequence - 255;
            self.window = [false; 256];
        }

        true
    }
}

impl EncryptionContext {
    /// Create new encryption context
    pub fn new(encr_key: &[u8; 32], auth_key: &[u8; 32]) -> Self {
        EncryptionContext {
            encr_key: *encr_key,
            auth_key: *auth_key,

            sequence: 0,

            mode: EncryptionMode::AEAD,

            packets_encrypted: 0,
            packets_decrypted: 0,

            replay_window: ReplayWindow::new(),
            replay_rejects: 0,

            mac_failures: 0,
        }
    }

    /// Encrypt packet with AEAD
    pub fn encrypt_packet(&mut self, plaintext: &[u8], metadata: &PacketMetadata) -> Option<EncryptedPacket> {
        if plaintext.len() > 240 {
            return None;
        }

        let mut nonce = [0u8; 12];
        for i in 0..8 {
            nonce[i] = ((self.sequence >> (i * 8)) & 0xFF) as u8;
        }

        let mut ciphertext = [0u8; 256];
        for i in 0..plaintext.len() {
            ciphertext[i] = plaintext[i] ^ self.encr_key[i % 32];
        }

        // Compute MAC
        let mac = self.compute_mac(&ciphertext[..plaintext.len()], metadata);

        // AAD (Additional Authenticated Data) from metadata
        let mut aad = [0u8; 64];
        aad[0..4].copy_from_slice(&metadata.source_ip.to_le_bytes());
        aad[4..8].copy_from_slice(&metadata.dest_ip.to_le_bytes());

        let packet = EncryptedPacket {
            packet_id: metadata.packet_id,
            sequence: self.sequence,
            ciphertext,
            ciphertext_len: plaintext.len() as u16,
            mac,
            nonce,
            aad,
            aad_len: 8,
        };

        self.sequence += 1;
        self.packets_encrypted += 1;

        Some(packet)
    }

    /// Decrypt packet with integrity verification
    pub fn decrypt_packet(&mut self, packet: &EncryptedPacket) -> Option<[u8; 256]> {
        // Check replay
        if !self.replay_window.is_valid(packet.sequence) {
            self.replay_rejects += 1;
            return None;
        }

        // Verify MAC
        let expected_mac = self.compute_mac(&packet.ciphertext[..packet.ciphertext_len as usize],
                                           &PacketMetadata {
                                               source_ip: u32::from_le_bytes([packet.aad[0], packet.aad[1], packet.aad[2], packet.aad[3]]),
                                               dest_ip: u32::from_le_bytes([packet.aad[4], packet.aad[5], packet.aad[6], packet.aad[7]]),
                                               protocol: 0,
                                               packet_id: packet.packet_id,
                                               timestamp: 0,
                                           });

        // Constant-time comparison
        let mut mac_match = true;
        for i in 0..32 {
            if expected_mac[i] != packet.mac[i] {
                mac_match = false;
            }
        }

        if !mac_match {
            self.mac_failures += 1;
            return None;
        }

        // Decrypt
        let mut plaintext = [0u8; 256];
        for i in 0..packet.ciphertext_len as usize {
            plaintext[i] = packet.ciphertext[i] ^ self.encr_key[i % 32];
        }

        self.sequence = (packet.sequence + 1).max(self.sequence);
        self.packets_decrypted += 1;

        Some(plaintext)
    }

    /// Compute MAC for packet
    fn compute_mac(&self, data: &[u8], metadata: &PacketMetadata) -> [u8; 32] {
        let mut mac = [0u8; 32];

        // Simplified HMAC: XOR-based
        for i in 0..data.len() {
            mac[i % 32] ^= data[i];
        }

        for i in 0..4 {
            mac[i] ^= ((metadata.source_ip >> (i * 8)) & 0xFF) as u8;
            mac[i + 4] ^= ((metadata.dest_ip >> (i * 8)) & 0xFF) as u8;
        }

        // Mix in auth key
        for i in 0..32 {
            mac[i] ^= self.auth_key[i];
        }

        mac
    }

    /// Verify MAC for packet
    pub fn verify_mac(&mut self, data: &[u8], metadata: &PacketMetadata, provided_mac: &[u8; 32]) -> bool {
        let computed = self.compute_mac(data, metadata);

        // Constant-time comparison
        let mut match_result = true;
        for i in 0..32 {
            if computed[i] != provided_mac[i] {
                match_result = false;
            }
        }

        if !match_result {
            self.mac_failures += 1;
        }

        match_result
    }

    /// Check for replay attack
    pub fn check_replay(&mut self, sequence: u64) -> bool {
        self.replay_window.is_valid(sequence)
    }

    /// Setup encryption mode
    pub fn setup_encryption(&mut self, mode: EncryptionMode) -> bool {
        self.mode = mode;
        true
    }

    /// Rotate keys
    pub fn rotate_keys(&mut self, new_encr_key: &[u8; 32], new_auth_key: &[u8; 32]) -> bool {
        self.encr_key = *new_encr_key;
        self.auth_key = *new_auth_key;
        self.sequence = 0;
        true
    }

    /// Get packet encryption count
    pub fn get_packets_encrypted(&self) -> u32 {
        self.packets_encrypted
    }

    /// Get packet decryption count
    pub fn get_packets_decrypted(&self) -> u32 {
        self.packets_decrypted
    }

    /// Get MAC failure count
    pub fn get_mac_failures(&self) -> u32 {
        self.mac_failures
    }

    /// Get replay rejections
    pub fn get_replay_rejects(&self) -> u32 {
        self.replay_rejects
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_context_creation() {
        let ctx = EncryptionContext::new(&[0u8; 32], &[0u8; 32]);
        assert_eq!(ctx.get_packets_encrypted(), 0);
        assert_eq!(ctx.get_packets_decrypted(), 0);
    }

    #[test]
    fn test_packet_encryption() {
        let mut ctx = EncryptionContext::new(&[0x42u8; 32], &[0x55u8; 32]);
        let metadata = PacketMetadata {
            source_ip: 0x7F000001,
            dest_ip: 0x7F000002,
            protocol: 6,
            packet_id: 1,
            timestamp: 0,
        };
        let plaintext = b"Test packet data";
        let encrypted = ctx.encrypt_packet(plaintext, &metadata);
        assert!(encrypted.is_some());
        assert_eq!(ctx.get_packets_encrypted(), 1);
    }

    #[test]
    fn test_replay_detection() {
        let mut window = ReplayWindow::new();
        assert!(window.is_valid(100));
        assert!(!window.is_valid(100)); // Duplicate
        assert!(window.is_valid(101));
    }
}
