//! Authentication & Authorization
//!
//! JWT token validation, role-based access control, and API key management.



/// Token type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenType {
    Jwt,
    ApiKey,
    Basic,
    Bearer,
}

/// User role
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Admin,
    ServiceAccount,
    User,
    Guest,
}

/// Permission type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    Read,
    Write,
    Execute,
    Admin,
}

/// Authentication token
#[derive(Clone, Copy)]
pub struct AuthToken {
    pub token_id: u32,
    pub user_id: u32,
    pub role: Role,
    pub permissions: u16,      // Bitmask: bit 0=Read, 1=Write, 2=Execute, 3=Admin
    pub created_at: u64,
    pub expires_at: u64,
    pub is_revoked: bool,
}

/// API key entry
#[derive(Clone, Copy)]
pub struct ApiKeyEntry {
    pub key_id: u32,
    pub service_id: u32,
    pub permissions: u16,      // Bitmask
    pub created_at: u64,
    pub expires_at: u64,
    pub is_revoked: bool,
}

/// Authentication manager
pub struct AuthenticationManager {
    tokens: [AuthToken; 256],
    token_count: u16,

    api_keys: [ApiKeyEntry; 512],
    key_count: u16,

    issued_tokens: u32,
    revoked_tokens: u16,
    failed_auth_attempts: u32,
}

impl AuthenticationManager {
    /// Create new authentication manager
    pub fn new() -> Self {
        AuthenticationManager {
            tokens: [AuthToken {
                token_id: 0,
                user_id: 0,
                role: Role::Guest,
                permissions: 0,
                created_at: 0,
                expires_at: 0,
                is_revoked: false,
            }; 256],
            token_count: 0,

            api_keys: [ApiKeyEntry {
                key_id: 0,
                service_id: 0,
                permissions: 0,
                created_at: 0,
                expires_at: 0,
                is_revoked: false,
            }; 512],
            key_count: 0,

            issued_tokens: 0,
            revoked_tokens: 0,
            failed_auth_attempts: 0,
        }
    }

    /// Issue a new token
    pub fn issue_token(&mut self, user_id: u32, role: Role, permissions: u16,
                      created_at: u64, expires_at: u64) -> Option<u32> {
        if (self.token_count as usize) >= 256 {
            return None;
        }

        let token_id = self.issued_tokens as u32;
        self.tokens[self.token_count as usize] = AuthToken {
            token_id,
            user_id,
            role,
            permissions,
            created_at,
            expires_at,
            is_revoked: false,
        };
        self.token_count += 1;
        self.issued_tokens += 1;
        Some(token_id)
    }

    /// Validate a token
    pub fn validate_token(&mut self, token_id: u32, current_time: u64) -> bool {
        for i in 0..(self.token_count as usize) {
            if self.tokens[i].token_id == token_id {
                let token = &self.tokens[i];

                // Check revocation
                if token.is_revoked {
                    self.failed_auth_attempts += 1;
                    return false;
                }

                // Check expiration
                if current_time > token.expires_at {
                    self.failed_auth_attempts += 1;
                    return false;
                }

                return true;
            }
        }

        self.failed_auth_attempts += 1;
        false
    }

    /// Register an API key
    pub fn register_api_key(&mut self, service_id: u32, permissions: u16,
                           created_at: u64, expires_at: u64) -> Option<u32> {
        if (self.key_count as usize) >= 512 {
            return None;
        }

        let key_id = self.key_count as u32;
        self.api_keys[self.key_count as usize] = ApiKeyEntry {
            key_id,
            service_id,
            permissions,
            created_at,
            expires_at,
            is_revoked: false,
        };
        self.key_count += 1;
        Some(key_id)
    }

    /// Verify an API key
    pub fn verify_api_key(&mut self, key_id: u32, current_time: u64) -> bool {
        for i in 0..(self.key_count as usize) {
            if self.api_keys[i].key_id == key_id {
                let key = &self.api_keys[i];

                // Check revocation
                if key.is_revoked {
                    self.failed_auth_attempts += 1;
                    return false;
                }

                // Check expiration
                if current_time > key.expires_at {
                    self.failed_auth_attempts += 1;
                    return false;
                }

                return true;
            }
        }

        self.failed_auth_attempts += 1;
        false
    }

    /// Grant a permission to a token
    pub fn grant_permission(&mut self, token_id: u32, permission: Permission) -> bool {
        for i in 0..(self.token_count as usize) {
            if self.tokens[i].token_id == token_id {
                let perm_bit = match permission {
                    Permission::Read => 1u16,
                    Permission::Write => 2u16,
                    Permission::Execute => 4u16,
                    Permission::Admin => 8u16,
                };
                self.tokens[i].permissions |= perm_bit;
                return true;
            }
        }
        false
    }

    /// Revoke a permission from a token
    pub fn revoke_permission(&mut self, token_id: u32, permission: Permission) -> bool {
        for i in 0..(self.token_count as usize) {
            if self.tokens[i].token_id == token_id {
                let perm_bit = match permission {
                    Permission::Read => 1u16,
                    Permission::Write => 2u16,
                    Permission::Execute => 4u16,
                    Permission::Admin => 8u16,
                };
                self.tokens[i].permissions &= !perm_bit;
                return true;
            }
        }
        false
    }

    /// Check if token has a permission
    pub fn check_permission(&self, token_id: u32, permission: Permission) -> bool {
        for i in 0..(self.token_count as usize) {
            if self.tokens[i].token_id == token_id {
                let perm_bit = match permission {
                    Permission::Read => 1u16,
                    Permission::Write => 2u16,
                    Permission::Execute => 4u16,
                    Permission::Admin => 8u16,
                };
                return (self.tokens[i].permissions & perm_bit) != 0;
            }
        }
        false
    }

    /// Refresh token expiration
    pub fn refresh_token(&mut self, token_id: u32, new_expiry: u64) -> bool {
        for i in 0..(self.token_count as usize) {
            if self.tokens[i].token_id == token_id {
                self.tokens[i].expires_at = new_expiry;
                return true;
            }
        }
        false
    }

    /// Revoke a token
    pub fn revoke_token(&mut self, token_id: u32) -> bool {
        for i in 0..(self.token_count as usize) {
            if self.tokens[i].token_id == token_id {
                self.tokens[i].is_revoked = true;
                self.revoked_tokens += 1;
                return true;
            }
        }
        false
    }

    /// Get user's roles
    pub fn get_user_role(&self, token_id: u32) -> Option<Role> {
        for i in 0..(self.token_count as usize) {
            if self.tokens[i].token_id == token_id {
                return Some(self.tokens[i].role);
            }
        }
        None
    }

    /// Get token count
    pub fn get_token_count(&self) -> u16 {
        self.token_count
    }

    /// Get API key count
    pub fn get_api_key_count(&self) -> u16 {
        self.key_count
    }

    /// Get revoked token count
    pub fn get_revoked_token_count(&self) -> u16 {
        self.revoked_tokens
    }

    /// Get failed auth attempts
    pub fn get_failed_auth_attempts(&self) -> u32 {
        self.failed_auth_attempts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_manager_creation() {
        let manager = AuthenticationManager::new();
        assert_eq!(manager.get_token_count(), 0);
        assert_eq!(manager.get_api_key_count(), 0);
    }

    #[test]
    fn test_token_issuance() {
        let mut manager = AuthenticationManager::new();
        let token_id = manager.issue_token(1, Role::User, 0x03, 0, 1000);
        assert!(token_id.is_some());
        assert_eq!(manager.get_token_count(), 1);
    }

    #[test]
    fn test_permission_checking() {
        let mut manager = AuthenticationManager::new();
        let token_id = manager.issue_token(1, Role::User, 0, 0, 1000).unwrap();
        manager.grant_permission(token_id, Permission::Read);
        assert!(manager.check_permission(token_id, Permission::Read));
        assert!(!manager.check_permission(token_id, Permission::Write));
    }
}
