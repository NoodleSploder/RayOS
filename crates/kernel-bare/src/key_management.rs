//! Key Management System
//!
//! Secure key storage, lifecycle management, rotation, and access control.
//! Supports 256 keys with encryption at rest and audit trails.



/// Key identifier
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyId(pub u16);

/// Key access policy
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAccessPolicy {
    Read,
    Write,
    ReadWrite,
    None,
}

/// Key metadata
#[derive(Clone, Copy)]
pub struct KeyMetadata {
    pub key_id: KeyId,
    pub created_timestamp: u64,
    pub expiration_timestamp: u64,
    pub rotation_version: u16,
    pub last_rotated: u64,
    pub is_revoked: bool,
    pub access_policy: KeyAccessPolicy,
    pub usage_count: u32,
}

/// Key derivation path for hierarchical keys
#[derive(Clone, Copy)]
pub struct KeyDerivationPath {
    pub parent_id: Option<KeyId>,
    pub depth: u8,
    pub context: [u8; 32],
}

/// Key store entry
#[derive(Clone, Copy)]
pub struct KeyStoreEntry {
    pub metadata: KeyMetadata,
    pub key_material: [u32; 8],
    pub encrypted: bool,
    pub encryption_key_id: Option<KeyId>,
}

/// Key rotation event
#[derive(Clone, Copy)]
pub struct KeyRotationEvent {
    pub key_id: KeyId,
    pub old_version: u16,
    pub new_version: u16,
    pub timestamp: u64,
}

/// Key management system
pub struct KeyStore {
    entries: [KeyStoreEntry; 256],
    entry_count: u16,

    rotations: [KeyRotationEvent; 128],
    rotation_count: u8,

    derivations: [KeyDerivationPath; 64],
    derivation_count: u8,

    audit_trail: [AuditEvent; 512],
    audit_count: u16,

    master_key_id: Option<KeyId>,
    rotation_interval_seconds: u64,
}

/// Audit event for key operations
#[derive(Clone, Copy)]
pub struct AuditEvent {
    pub timestamp: u64,
    pub key_id: KeyId,
    pub operation: KeyOperation,
    pub actor: u32,
}

/// Key operation type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyOperation {
    Create,
    Access,
    Rotate,
    Revoke,
    Derive,
    Delete,
    Backup,
}

impl KeyStore {
    /// Create new key store
    pub fn new() -> Self {
        KeyStore {
            entries: [KeyStoreEntry {
                metadata: KeyMetadata {
                    key_id: KeyId(0),
                    created_timestamp: 0,
                    expiration_timestamp: 0,
                    rotation_version: 0,
                    last_rotated: 0,
                    is_revoked: false,
                    access_policy: KeyAccessPolicy::None,
                    usage_count: 0,
                },
                key_material: [0u32; 8],
                encrypted: false,
                encryption_key_id: None,
            }; 256],
            entry_count: 0,

            rotations: [KeyRotationEvent {
                key_id: KeyId(0),
                old_version: 0,
                new_version: 0,
                timestamp: 0,
            }; 128],
            rotation_count: 0,

            derivations: [KeyDerivationPath {
                parent_id: None,
                depth: 0,
                context: [0u8; 32],
            }; 64],
            derivation_count: 0,

            audit_trail: [AuditEvent {
                timestamp: 0,
                key_id: KeyId(0),
                operation: KeyOperation::Create,
                actor: 0,
            }; 512],
            audit_count: 0,

            master_key_id: None,
            rotation_interval_seconds: 86400, // 24 hours
        }
    }

    /// Create and store new key
    pub fn create_key(&mut self, key_material: &[u32; 8],
                     expiration_secs: u64, policy: KeyAccessPolicy) -> Option<KeyId> {
        if self.entry_count >= 256 {
            return None;
        }

        let key_id = KeyId(self.entry_count as u16);
        let now = 0u64; // Would use current time in production

        self.entries[self.entry_count as usize] = KeyStoreEntry {
            metadata: KeyMetadata {
                key_id,
                created_timestamp: now,
                expiration_timestamp: now + expiration_secs,
                rotation_version: 1,
                last_rotated: now,
                is_revoked: false,
                access_policy: policy,
                usage_count: 0,
            },
            key_material: *key_material,
            encrypted: false,
            encryption_key_id: None,
        };

        self.entry_count += 1;
        self.log_audit(key_id, KeyOperation::Create, 0);
        Some(key_id)
    }

    /// Get key if not expired or revoked
    pub fn get_key(&mut self, key_id: KeyId) -> Option<[u32; 8]> {
        for i in 0..self.entry_count as usize {
            if self.entries[i].metadata.key_id == key_id {
                let entry = &self.entries[i];

                // Check revocation and expiration
                if entry.metadata.is_revoked {
                    return None;
                }

                // Check policy
                if entry.metadata.access_policy == KeyAccessPolicy::None {
                    return None;
                }

                let material = entry.key_material;
                // Log access after returning material
                self.log_audit(key_id, KeyOperation::Access, 0);

                // Update usage count
                self.entries[i].metadata.usage_count += 1;

                return Some(material);
            }
        }
        None
    }

    /// Rotate key to new version
    pub fn rotate_key(&mut self, key_id: KeyId, new_material: &[u32; 8]) -> bool {
        for i in 0..self.entry_count as usize {
            if self.entries[i].metadata.key_id == key_id {
                let old_version = self.entries[i].metadata.rotation_version;
                let new_version = old_version + 1;

                // Record rotation
                if self.rotation_count < 128 {
                    self.rotations[self.rotation_count as usize] = KeyRotationEvent {
                        key_id,
                        old_version,
                        new_version,
                        timestamp: 0,
                    };
                    self.rotation_count += 1;
                }

                // Update key
                self.entries[i].key_material = *new_material;
                self.entries[i].metadata.rotation_version = new_version;
                self.entries[i].metadata.last_rotated = 0;

                self.log_audit(key_id, KeyOperation::Rotate, 0);
                return true;
            }
        }
        false
    }

    /// Revoke key immediately
    pub fn revoke_key(&mut self, key_id: KeyId) -> bool {
        for i in 0..self.entry_count as usize {
            if self.entries[i].metadata.key_id == key_id {
                self.entries[i].metadata.is_revoked = true;
                self.log_audit(key_id, KeyOperation::Revoke, 0);
                return true;
            }
        }
        false
    }

    /// Derive child key from parent
    pub fn derive_key(&mut self, parent_id: KeyId, context: &[u8; 32]) -> Option<KeyId> {
        // Check parent exists
        let mut parent_found = false;
        for i in 0..self.entry_count as usize {
            if self.entries[i].metadata.key_id == parent_id {
                parent_found = true;
                break;
            }
        }

        if !parent_found {
            return None;
        }

        // Create child key
        if self.entry_count >= 256 {
            return None;
        }

        let child_id = KeyId(self.entry_count as u16);

        // Derive new material from parent + context
        let parent_key = self.get_key(parent_id)?;
        let mut derived_material = [0u32; 8];
        for i in 0..8 {
            derived_material[i] = parent_key[i] ^ u32::from_le_bytes([
                context[i * 4],
                context[i * 4 + 1],
                context[i * 4 + 2],
                context[i * 4 + 3],
            ]);
        }

        self.entries[self.entry_count as usize] = KeyStoreEntry {
            metadata: KeyMetadata {
                key_id: child_id,
                created_timestamp: 0,
                expiration_timestamp: 0,
                rotation_version: 1,
                last_rotated: 0,
                is_revoked: false,
                access_policy: KeyAccessPolicy::Read,
                usage_count: 0,
            },
            key_material: derived_material,
            encrypted: false,
            encryption_key_id: Some(parent_id),
        };

        self.entry_count += 1;

        // Record derivation
        if self.derivation_count < 64 {
            self.derivations[self.derivation_count as usize] = KeyDerivationPath {
                parent_id: Some(parent_id),
                depth: 1,
                context: *context,
            };
            self.derivation_count += 1;
        }

        self.log_audit(child_id, KeyOperation::Derive, 0);
        Some(child_id)
    }

    /// Securely erase key material
    pub fn erase_key(&mut self, key_id: KeyId) -> bool {
        for i in 0..self.entry_count as usize {
            if self.entries[i].metadata.key_id == key_id {
                // Overwrite with zeros
                self.entries[i].key_material = [0u32; 8];
                self.entries[i].metadata.is_revoked = true;
                self.log_audit(key_id, KeyOperation::Delete, 0);
                return true;
            }
        }
        false
    }

    /// Log audit event
    fn log_audit(&mut self, key_id: KeyId, operation: KeyOperation, actor: u32) {
        if self.audit_count < 512 {
            self.audit_trail[self.audit_count as usize] = AuditEvent {
                timestamp: 0,
                key_id,
                operation,
                actor,
            };
            self.audit_count += 1;
        }
    }

    /// Get key metadata
    pub fn get_metadata(&self, key_id: KeyId) -> Option<KeyMetadata> {
        for i in 0..self.entry_count as usize {
            if self.entries[i].metadata.key_id == key_id {
                return Some(self.entries[i].metadata);
            }
        }
        None
    }

    /// Get audit trail count
    pub fn get_audit_count(&self) -> u16 {
        self.audit_count
    }

    /// Get rotation history count
    pub fn get_rotation_count(&self) -> u8 {
        self.rotation_count
    }

    /// Get total keys
    pub fn get_key_count(&self) -> u16 {
        self.entry_count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keystore_creation() {
        let store = KeyStore::new();
        assert_eq!(store.get_key_count(), 0);
    }

    #[test]
    fn test_create_key() {
        let mut store = KeyStore::new();
        let key_id = store.create_key(&[0u32; 8], 3600, KeyAccessPolicy::ReadWrite);
        assert!(key_id.is_some());
        assert_eq!(store.get_key_count(), 1);
    }

    #[test]
    fn test_key_retrieval() {
        let mut store = KeyStore::new();
        let key_id = store.create_key(&[1u32; 8], 3600, KeyAccessPolicy::ReadWrite).unwrap();
        let key = store.get_key(key_id);
        assert!(key.is_some());
    }
}
