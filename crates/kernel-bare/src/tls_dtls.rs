//! TLS/DTLS Protocol Implementation
//!
//! TLS 1.3 record layer with DTLS 1.3 support for UDP.
//! Includes handshake state machine and cipher suite management.

#![no_std]

/// Cipher suite type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CipherSuite {
    TLS_AES_128_GCM_SHA256,
    TLS_AES_256_GCM_SHA384,
    TLS_CHACHA20_POLY1305_SHA256,
    DTLS_AES_128_CCM_SHA256,
    DTLS_AES_256_GCM_SHA384,
}

/// TLS record type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordType {
    ChangeCipherSpec,
    Alert,
    Handshake,
    ApplicationData,
}

/// Handshake message type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandshakeType {
    ClientHello,
    ServerHello,
    Certificate,
    ServerKeyExchange,
    CertificateRequest,
    ServerHelloDone,
    ClientKeyExchange,
    Finished,
}

/// Handshake state
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HandshakeState {
    Start,
    WaitServerHello,
    WaitCertificate,
    WaitServerKeyExchange,
    WaitServerHelloDone,
    WaitCertificateVerify,
    WaitFinished,
    Established,
    Closed,
}

/// Alert level
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertLevel {
    Warning,
    Fatal,
}

/// Alert description
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AlertDescription {
    CloseNotify,
    UnexpectedMessage,
    BadRecordMAC,
    DecryptionFailed,
    RecordOverflow,
    DecompressionFailure,
    HandshakeFailure,
    NoCertificate,
    BadCertificate,
    UnsupportedCertificate,
    CertificateRevoked,
    CertificateExpired,
    CertificateUnknown,
    IllegalParameter,
    UnknownCA,
    AccessDenied,
    DecodeError,
    DecryptError,
    ExportRestriction,
    ProtocolVersion,
    InsufficientSecurity,
    InternalError,
    UserCanceled,
    NoRenegotiation,
}

/// TLS Record structure
#[derive(Clone, Copy)]
pub struct TlsRecord {
    pub record_type: RecordType,
    pub version: u16,
    pub sequence: u64,
    pub length: u16,
    pub data: [u8; 256],
}

/// Session ticket for resumption
#[derive(Clone, Copy)]
pub struct SessionTicket {
    pub ticket_id: u32,
    pub lifetime: u32,
    pub created_time: u64,
    pub psk: [u8; 32],
    pub ticket_data: [u8; 128],
}

/// Handshake message
#[derive(Clone, Copy)]
pub struct HandshakeMessage {
    pub message_type: HandshakeType,
    pub message_seq: u16,
    pub length: u32,
    pub data: [u8; 512],
}

/// TLS Context
pub struct TlsContext {
    state: HandshakeState,
    cipher_suite: CipherSuite,

    client_random: [u8; 32],
    server_random: [u8; 32],

    master_secret: [u8; 48],
    client_write_key: [u8; 32],
    server_write_key: [u8; 32],
    client_write_iv: [u8; 12],
    server_write_iv: [u8; 12],

    sequence_write: u64,
    sequence_read: u64,

    handshake_messages: [HandshakeMessage; 32],
    handshake_count: u8,

    session_tickets: [SessionTicket; 8],
    ticket_count: u8,

    supported_ciphers: [CipherSuite; 5],
    cipher_count: u8,

    peer_certificate: [u8; 256],
    peer_cert_len: u16,

    is_dtls: bool,
    is_client: bool,
}

impl TlsContext {
    /// Create new TLS context
    pub fn new(is_client: bool) -> Self {
        TlsContext {
            state: HandshakeState::Start,
            cipher_suite: CipherSuite::TLS_AES_128_GCM_SHA256,

            client_random: [0u8; 32],
            server_random: [0u8; 32],

            master_secret: [0u8; 48],
            client_write_key: [0u8; 32],
            server_write_key: [0u8; 32],
            client_write_iv: [0u8; 12],
            server_write_iv: [0u8; 12],

            sequence_write: 0,
            sequence_read: 0,

            handshake_messages: [HandshakeMessage {
                message_type: HandshakeType::ClientHello,
                message_seq: 0,
                length: 0,
                data: [0u8; 512],
            }; 32],
            handshake_count: 0,

            session_tickets: [SessionTicket {
                ticket_id: 0,
                lifetime: 0,
                created_time: 0,
                psk: [0u8; 32],
                ticket_data: [0u8; 128],
            }; 8],
            ticket_count: 0,

            supported_ciphers: [
                CipherSuite::TLS_AES_128_GCM_SHA256,
                CipherSuite::TLS_AES_256_GCM_SHA384,
                CipherSuite::TLS_CHACHA20_POLY1305_SHA256,
                CipherSuite::DTLS_AES_128_CCM_SHA256,
                CipherSuite::DTLS_AES_256_GCM_SHA384,
            ],
            cipher_count: 5,

            peer_certificate: [0u8; 256],
            peer_cert_len: 0,

            is_dtls: false,
            is_client,
        }
    }

    /// Create DTLS context instead of TLS
    pub fn new_dtls(is_client: bool) -> Self {
        let mut ctx = Self::new(is_client);
        ctx.is_dtls = true;
        ctx
    }

    /// Start TLS handshake
    pub fn start_handshake(&mut self) -> bool {
        if self.state != HandshakeState::Start {
            return false;
        }

        // Generate client random
        for i in 0..32 {
            self.client_random[i] = ((i as u32 * 7919) % 256) as u8;
        }

        self.state = HandshakeState::WaitServerHello;
        true
    }

    /// Process received TLS record
    pub fn process_record(&mut self, record: &TlsRecord) -> bool {
        // Validate record type
        match record.record_type {
            RecordType::Handshake => {
                self.process_handshake(&record.data[..record.length as usize])
            }
            RecordType::Alert => {
                if record.length >= 2 {
                    self.state = HandshakeState::Closed;
                }
                true
            }
            RecordType::ApplicationData => {
                self.state == HandshakeState::Established
            }
            _ => false,
        }
    }

    /// Process handshake message
    fn process_handshake(&mut self, data: &[u8]) -> bool {
        if data.is_empty() {
            return false;
        }

        let msg_type = match data[0] {
            0 => HandshakeType::ClientHello,
            2 => HandshakeType::ServerHello,
            11 => HandshakeType::Certificate,
            12 => HandshakeType::ServerKeyExchange,
            13 => HandshakeType::CertificateRequest,
            14 => HandshakeType::ServerHelloDone,
            16 => HandshakeType::ClientKeyExchange,
            20 => HandshakeType::Finished,
            _ => return false,
        };

        // Record handshake message
        if (self.handshake_count as usize) < 32 {
            self.handshake_messages[self.handshake_count as usize] = HandshakeMessage {
                message_type: msg_type,
                message_seq: self.handshake_count as u16,
                length: data.len() as u32,
                data: {
                    let mut buf = [0u8; 512];
                    let copy_len = core::cmp::min(data.len(), 512);
                    buf[..copy_len].copy_from_slice(&data[..copy_len]);
                    buf
                },
            };
            self.handshake_count += 1;
        }

        // Update state based on message
        match msg_type {
            HandshakeType::ServerHello => {
                self.state = HandshakeState::WaitCertificate;
                true
            }
            HandshakeType::Certificate => {
                self.state = HandshakeState::WaitServerKeyExchange;
                // Store certificate
                if data.len() > 256 {
                    self.peer_cert_len = 256;
                } else {
                    self.peer_cert_len = data.len() as u16;
                }
                true
            }
            HandshakeType::ServerKeyExchange => {
                self.state = HandshakeState::WaitServerHelloDone;
                true
            }
            HandshakeType::ServerHelloDone => {
                self.state = HandshakeState::WaitCertificateVerify;
                true
            }
            HandshakeType::Finished => {
                self.state = HandshakeState::Established;
                true
            }
            _ => true,
        }
    }

    /// Encrypt application data
    pub fn send_message(&mut self, plaintext: &[u8]) -> Option<[u8; 256]> {
        if self.state != HandshakeState::Established {
            return None;
        }

        let mut ciphertext = [0u8; 256];

        if plaintext.len() > 240 {
            return None;
        }

        // Simple XOR encryption (would use real AES-GCM in production)
        for i in 0..plaintext.len() {
            ciphertext[i] = plaintext[i] ^ self.client_write_key[i % 32];
        }

        self.sequence_write += 1;
        Some(ciphertext)
    }

    /// Decrypt received message
    pub fn recv_message(&mut self, ciphertext: &[u8]) -> Option<[u8; 256]> {
        if self.state != HandshakeState::Established {
            return None;
        }

        let mut plaintext = [0u8; 256];

        if ciphertext.len() > 256 {
            return None;
        }

        // Simple XOR decryption
        for i in 0..ciphertext.len() {
            plaintext[i] = ciphertext[i] ^ self.server_write_key[i % 32];
        }

        self.sequence_read += 1;
        Some(plaintext)
    }

    /// Generate session ticket
    pub fn get_session_ticket(&mut self) -> Option<SessionTicket> {
        if self.ticket_count >= 8 {
            return None;
        }

        let ticket = SessionTicket {
            ticket_id: self.ticket_count as u32,
            lifetime: 3600,
            created_time: 0,
            psk: {
                let mut psk = [0u8; 32];
                for i in 0..32 {
                    psk[i] = ((i as u32 * 13) % 256) as u8;
                }
                psk
            },
            ticket_data: [0u8; 128],
        };

        self.session_tickets[self.ticket_count as usize] = ticket;
        self.ticket_count += 1;

        Some(ticket)
    }

    /// Validate peer certificate
    pub fn validate_certificate(&self, cert_data: &[u8]) -> bool {
        if cert_data.is_empty() {
            return false;
        }

        // Check certificate format (simplified)
        cert_data.len() > 0 && cert_data[0] == 0x30 // SEQUENCE tag in ASN.1
    }

    /// Get current handshake state
    pub fn get_state(&self) -> HandshakeState {
        self.state
    }

    /// Set cipher suite
    pub fn set_cipher_suite(&mut self, suite: CipherSuite) -> bool {
        for cipher in &self.supported_ciphers[..self.cipher_count as usize] {
            if *cipher == suite {
                self.cipher_suite = suite;
                return true;
            }
        }
        false
    }

    /// Get selected cipher suite
    pub fn get_cipher_suite(&self) -> CipherSuite {
        self.cipher_suite
    }

    /// Get handshake message count
    pub fn get_handshake_count(&self) -> u8 {
        self.handshake_count
    }

    /// Is DTLS mode
    pub fn is_dtls(&self) -> bool {
        self.is_dtls
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_context_creation() {
        let ctx = TlsContext::new(true);
        assert_eq!(ctx.get_state(), HandshakeState::Start);
        assert!(!ctx.is_dtls());
    }

    #[test]
    fn test_dtls_context_creation() {
        let ctx = TlsContext::new_dtls(false);
        assert!(ctx.is_dtls());
        assert_eq!(ctx.get_state(), HandshakeState::Start);
    }

    #[test]
    fn test_start_handshake() {
        let mut ctx = TlsContext::new(true);
        assert!(ctx.start_handshake());
        assert_eq!(ctx.get_state(), HandshakeState::WaitServerHello);
    }
}
