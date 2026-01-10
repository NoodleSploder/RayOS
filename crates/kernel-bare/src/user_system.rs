// ===== RayOS User/Group System & Permission Enforcement (Phase 9B Task 5) =====
// User accounts, groups, ACLs, permission checking, capabilities

use core::sync::atomic::{AtomicU32, Ordering};

// ===== Constants =====

const MAX_USERS: usize = 256;
const MAX_GROUPS: usize = 128;
const MAX_GROUPS_PER_USER: usize = 32;
const MAX_USERNAME_LEN: usize = 32;
const MAX_GROUPNAME_LEN: usize = 32;
const MAX_ACL_ENTRIES: usize = 64;
const MAX_SESSIONS: usize = 64;

// ===== Special User/Group IDs =====

pub const UID_ROOT: u32 = 0;
pub const UID_NOBODY: u32 = 65534;
pub const UID_SYSTEM: u32 = 1;

pub const GID_ROOT: u32 = 0;
pub const GID_WHEEL: u32 = 10;
pub const GID_USERS: u32 = 100;
pub const GID_NOBODY: u32 = 65534;

// ===== User Flags =====

#[derive(Debug, Copy, Clone)]
pub struct UserFlags {
    /// Account is enabled
    pub enabled: bool,
    /// Account is locked
    pub locked: bool,
    /// Password expired
    pub password_expired: bool,
    /// Must change password
    pub must_change_password: bool,
    /// Can use sudo
    pub can_sudo: bool,
    /// Is system account
    pub system_account: bool,
    /// Shell access
    pub shell_access: bool,
    /// Can login remotely
    pub remote_login: bool,
}

impl UserFlags {
    pub fn default_user() -> Self {
        UserFlags {
            enabled: true,
            locked: false,
            password_expired: false,
            must_change_password: false,
            can_sudo: false,
            system_account: false,
            shell_access: true,
            remote_login: true,
        }
    }

    pub fn system_account() -> Self {
        UserFlags {
            enabled: true,
            locked: false,
            password_expired: false,
            must_change_password: false,
            can_sudo: false,
            system_account: true,
            shell_access: false,
            remote_login: false,
        }
    }

    pub fn root_account() -> Self {
        UserFlags {
            enabled: true,
            locked: false,
            password_expired: false,
            must_change_password: false,
            can_sudo: true,
            system_account: false,
            shell_access: true,
            remote_login: true,
        }
    }
}

// ===== User Account =====

#[derive(Copy, Clone)]
pub struct User {
    /// User ID
    pub uid: u32,
    /// Primary group ID
    pub gid: u32,
    /// Username
    username: [u8; MAX_USERNAME_LEN],
    username_len: usize,
    /// Password hash (truncated for demo)
    password_hash: [u8; 64],
    /// User flags
    pub flags: UserFlags,
    /// Home directory inode
    pub home_inode: u64,
    /// Shell path hash
    pub shell_hash: u64,
    /// Account creation time
    pub created_at: u64,
    /// Last login time
    pub last_login: u64,
    /// Failed login count
    pub failed_logins: u32,
    /// Supplementary groups
    groups: [u32; MAX_GROUPS_PER_USER],
    group_count: usize,
}

impl User {
    pub fn new(uid: u32, gid: u32) -> Self {
        User {
            uid,
            gid,
            username: [0u8; MAX_USERNAME_LEN],
            username_len: 0,
            password_hash: [0u8; 64],
            flags: UserFlags::default_user(),
            home_inode: 0,
            shell_hash: 0,
            created_at: 0,
            last_login: 0,
            failed_logins: 0,
            groups: [0u32; MAX_GROUPS_PER_USER],
            group_count: 0,
        }
    }

    pub fn set_username(&mut self, name: &str) {
        let len = core::cmp::min(name.len(), MAX_USERNAME_LEN - 1);
        for i in 0..len {
            self.username[i] = name.as_bytes()[i];
        }
        self.username_len = len;
    }

    pub fn username(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.username[..self.username_len]) }
    }

    pub fn set_password_hash(&mut self, hash: &[u8]) {
        let len = core::cmp::min(hash.len(), 64);
        for i in 0..len {
            self.password_hash[i] = hash[i];
        }
    }

    pub fn verify_password(&self, hash: &[u8]) -> bool {
        if hash.len() != 64 {
            return false;
        }
        for i in 0..64 {
            if self.password_hash[i] != hash[i] {
                return false;
            }
        }
        true
    }

    pub fn add_group(&mut self, gid: u32) -> bool {
        if self.group_count >= MAX_GROUPS_PER_USER {
            return false;
        }
        // Check if already in group
        for i in 0..self.group_count {
            if self.groups[i] == gid {
                return true;
            }
        }
        self.groups[self.group_count] = gid;
        self.group_count += 1;
        true
    }

    pub fn remove_group(&mut self, gid: u32) -> bool {
        for i in 0..self.group_count {
            if self.groups[i] == gid {
                for j in i..self.group_count - 1 {
                    self.groups[j] = self.groups[j + 1];
                }
                self.group_count -= 1;
                return true;
            }
        }
        false
    }

    pub fn is_in_group(&self, gid: u32) -> bool {
        if self.gid == gid {
            return true;
        }
        for i in 0..self.group_count {
            if self.groups[i] == gid {
                return true;
            }
        }
        false
    }

    pub fn groups(&self) -> &[u32] {
        &self.groups[..self.group_count]
    }

    pub fn is_root(&self) -> bool {
        self.uid == UID_ROOT
    }

    pub fn can_login(&self) -> bool {
        self.flags.enabled && !self.flags.locked && !self.flags.system_account
    }
}

// ===== Group =====

#[derive(Copy, Clone)]
pub struct Group {
    /// Group ID
    pub gid: u32,
    /// Group name
    name: [u8; MAX_GROUPNAME_LEN],
    name_len: usize,
    /// Is system group
    pub system_group: bool,
    /// Member UIDs (simplified)
    members: [u32; MAX_GROUPS_PER_USER],
    member_count: usize,
}

impl Group {
    pub fn new(gid: u32) -> Self {
        Group {
            gid,
            name: [0u8; MAX_GROUPNAME_LEN],
            name_len: 0,
            system_group: false,
            members: [0u32; MAX_GROUPS_PER_USER],
            member_count: 0,
        }
    }

    pub fn set_name(&mut self, name: &str) {
        let len = core::cmp::min(name.len(), MAX_GROUPNAME_LEN - 1);
        for i in 0..len {
            self.name[i] = name.as_bytes()[i];
        }
        self.name_len = len;
    }

    pub fn name(&self) -> &str {
        unsafe { core::str::from_utf8_unchecked(&self.name[..self.name_len]) }
    }

    pub fn add_member(&mut self, uid: u32) -> bool {
        if self.member_count >= MAX_GROUPS_PER_USER {
            return false;
        }
        for i in 0..self.member_count {
            if self.members[i] == uid {
                return true;
            }
        }
        self.members[self.member_count] = uid;
        self.member_count += 1;
        true
    }

    pub fn remove_member(&mut self, uid: u32) -> bool {
        for i in 0..self.member_count {
            if self.members[i] == uid {
                for j in i..self.member_count - 1 {
                    self.members[j] = self.members[j + 1];
                }
                self.member_count -= 1;
                return true;
            }
        }
        false
    }

    pub fn has_member(&self, uid: u32) -> bool {
        for i in 0..self.member_count {
            if self.members[i] == uid {
                return true;
            }
        }
        false
    }
}

// ===== File Permissions =====

#[derive(Debug, Copy, Clone)]
pub struct FileMode {
    /// Raw mode bits (Unix-style)
    pub mode: u16,
}

impl FileMode {
    // Permission bits
    pub const S_IRUSR: u16 = 0o400;  // Owner read
    pub const S_IWUSR: u16 = 0o200;  // Owner write
    pub const S_IXUSR: u16 = 0o100;  // Owner execute
    pub const S_IRGRP: u16 = 0o040;  // Group read
    pub const S_IWGRP: u16 = 0o020;  // Group write
    pub const S_IXGRP: u16 = 0o010;  // Group execute
    pub const S_IROTH: u16 = 0o004;  // Other read
    pub const S_IWOTH: u16 = 0o002;  // Other write
    pub const S_IXOTH: u16 = 0o001;  // Other execute
    
    // Special bits
    pub const S_ISUID: u16 = 0o4000; // Set UID
    pub const S_ISGID: u16 = 0o2000; // Set GID
    pub const S_ISVTX: u16 = 0o1000; // Sticky bit

    pub const fn new(mode: u16) -> Self {
        FileMode { mode }
    }

    pub const fn default_file() -> Self {
        FileMode { mode: 0o644 }
    }

    pub const fn default_dir() -> Self {
        FileMode { mode: 0o755 }
    }

    pub const fn default_exec() -> Self {
        FileMode { mode: 0o755 }
    }

    pub fn owner_read(&self) -> bool { self.mode & Self::S_IRUSR != 0 }
    pub fn owner_write(&self) -> bool { self.mode & Self::S_IWUSR != 0 }
    pub fn owner_exec(&self) -> bool { self.mode & Self::S_IXUSR != 0 }
    pub fn group_read(&self) -> bool { self.mode & Self::S_IRGRP != 0 }
    pub fn group_write(&self) -> bool { self.mode & Self::S_IWGRP != 0 }
    pub fn group_exec(&self) -> bool { self.mode & Self::S_IXGRP != 0 }
    pub fn other_read(&self) -> bool { self.mode & Self::S_IROTH != 0 }
    pub fn other_write(&self) -> bool { self.mode & Self::S_IWOTH != 0 }
    pub fn other_exec(&self) -> bool { self.mode & Self::S_IXOTH != 0 }
    pub fn setuid(&self) -> bool { self.mode & Self::S_ISUID != 0 }
    pub fn setgid(&self) -> bool { self.mode & Self::S_ISGID != 0 }
    pub fn sticky(&self) -> bool { self.mode & Self::S_ISVTX != 0 }
}

// ===== ACL Entry =====

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum AclTag {
    /// Owner permissions
    UserObj,
    /// Named user
    User,
    /// Owning group
    GroupObj,
    /// Named group
    Group,
    /// ACL mask
    Mask,
    /// Other permissions
    Other,
}

#[derive(Copy, Clone)]
pub struct AclEntry {
    /// ACL tag type
    pub tag: AclTag,
    /// User/Group ID (for User/Group tags)
    pub id: u32,
    /// Read permission
    pub read: bool,
    /// Write permission
    pub write: bool,
    /// Execute permission
    pub execute: bool,
}

impl AclEntry {
    pub fn new(tag: AclTag, id: u32, read: bool, write: bool, execute: bool) -> Self {
        AclEntry { tag, id, read, write, execute }
    }

    pub fn perm_bits(&self) -> u8 {
        let mut bits = 0u8;
        if self.read { bits |= 4; }
        if self.write { bits |= 2; }
        if self.execute { bits |= 1; }
        bits
    }
}

// ===== Access Control List =====

#[derive(Copy, Clone)]
pub struct Acl {
    entries: [AclEntry; MAX_ACL_ENTRIES],
    entry_count: usize,
}

impl Acl {
    pub fn new() -> Self {
        Acl {
            entries: [AclEntry::new(AclTag::Other, 0, false, false, false); MAX_ACL_ENTRIES],
            entry_count: 0,
        }
    }

    pub fn add_entry(&mut self, entry: AclEntry) -> bool {
        if self.entry_count >= MAX_ACL_ENTRIES {
            return false;
        }
        self.entries[self.entry_count] = entry;
        self.entry_count += 1;
        true
    }

    pub fn find_user(&self, uid: u32) -> Option<&AclEntry> {
        for i in 0..self.entry_count {
            if self.entries[i].tag == AclTag::User && self.entries[i].id == uid {
                return Some(&self.entries[i]);
            }
        }
        None
    }

    pub fn find_group(&self, gid: u32) -> Option<&AclEntry> {
        for i in 0..self.entry_count {
            if self.entries[i].tag == AclTag::Group && self.entries[i].id == gid {
                return Some(&self.entries[i]);
            }
        }
        None
    }

    pub fn get_mask(&self) -> Option<&AclEntry> {
        for i in 0..self.entry_count {
            if self.entries[i].tag == AclTag::Mask {
                return Some(&self.entries[i]);
            }
        }
        None
    }

    pub fn entries(&self) -> &[AclEntry] {
        &self.entries[..self.entry_count]
    }
}

// ===== Permission Request =====

#[derive(Debug, Copy, Clone)]
pub struct PermissionRequest {
    /// Requesting user ID
    pub uid: u32,
    /// Requesting group ID
    pub gid: u32,
    /// Supplementary groups (bitmask for efficiency, up to 64)
    pub groups_mask: u64,
    /// Want read access
    pub want_read: bool,
    /// Want write access
    pub want_write: bool,
    /// Want execute access
    pub want_execute: bool,
}

impl PermissionRequest {
    pub fn read(uid: u32, gid: u32) -> Self {
        PermissionRequest {
            uid, gid,
            groups_mask: 0,
            want_read: true,
            want_write: false,
            want_execute: false,
        }
    }

    pub fn write(uid: u32, gid: u32) -> Self {
        PermissionRequest {
            uid, gid,
            groups_mask: 0,
            want_read: false,
            want_write: true,
            want_execute: false,
        }
    }

    pub fn execute(uid: u32, gid: u32) -> Self {
        PermissionRequest {
            uid, gid,
            groups_mask: 0,
            want_read: false,
            want_write: false,
            want_execute: true,
        }
    }

    pub fn rw(uid: u32, gid: u32) -> Self {
        PermissionRequest {
            uid, gid,
            groups_mask: 0,
            want_read: true,
            want_write: true,
            want_execute: false,
        }
    }
}

// ===== File Ownership =====

#[derive(Copy, Clone)]
pub struct FileOwnership {
    /// Owner user ID
    pub uid: u32,
    /// Owner group ID
    pub gid: u32,
    /// File mode
    pub mode: FileMode,
    /// Optional ACL
    pub acl: Option<Acl>,
}

impl FileOwnership {
    pub fn new(uid: u32, gid: u32, mode: FileMode) -> Self {
        FileOwnership { uid, gid, mode, acl: None }
    }

    /// Check if request is permitted
    pub fn check_permission(&self, req: &PermissionRequest) -> bool {
        // Root can do anything
        if req.uid == UID_ROOT {
            return true;
        }

        // Check ACL first if present
        if let Some(ref acl) = self.acl {
            return self.check_acl_permission(acl, req);
        }

        // Standard Unix permission check
        self.check_mode_permission(req)
    }

    fn check_mode_permission(&self, req: &PermissionRequest) -> bool {
        // Owner?
        if req.uid == self.uid {
            if req.want_read && !self.mode.owner_read() { return false; }
            if req.want_write && !self.mode.owner_write() { return false; }
            if req.want_execute && !self.mode.owner_exec() { return false; }
            return true;
        }

        // Group member?
        let in_group = req.gid == self.gid || 
                       (self.gid < 64 && (req.groups_mask & (1u64 << self.gid)) != 0);
        
        if in_group {
            if req.want_read && !self.mode.group_read() { return false; }
            if req.want_write && !self.mode.group_write() { return false; }
            if req.want_execute && !self.mode.group_exec() { return false; }
            return true;
        }

        // Other
        if req.want_read && !self.mode.other_read() { return false; }
        if req.want_write && !self.mode.other_write() { return false; }
        if req.want_execute && !self.mode.other_exec() { return false; }
        true
    }

    fn check_acl_permission(&self, acl: &Acl, req: &PermissionRequest) -> bool {
        // Check named user entry
        if let Some(entry) = acl.find_user(req.uid) {
            return self.check_acl_entry(entry, acl.get_mask(), req);
        }

        // Check owner
        if req.uid == self.uid {
            // Use UserObj permissions
            for e in acl.entries() {
                if e.tag == AclTag::UserObj {
                    return self.check_acl_entry(e, None, req);
                }
            }
        }

        // Check named group entries
        for entry in acl.entries() {
            if entry.tag == AclTag::Group {
                let in_group = entry.id == req.gid || 
                              (entry.id < 64 && (req.groups_mask & (1u64 << entry.id)) != 0);
                if in_group {
                    return self.check_acl_entry(entry, acl.get_mask(), req);
                }
            }
        }

        // Check owning group
        if req.gid == self.gid {
            for e in acl.entries() {
                if e.tag == AclTag::GroupObj {
                    return self.check_acl_entry(e, acl.get_mask(), req);
                }
            }
        }

        // Fall back to Other
        for e in acl.entries() {
            if e.tag == AclTag::Other {
                return self.check_acl_entry(e, None, req);
            }
        }

        false
    }

    fn check_acl_entry(&self, entry: &AclEntry, mask: Option<&AclEntry>, req: &PermissionRequest) -> bool {
        let (r, w, x) = if let Some(m) = mask {
            (entry.read && m.read, entry.write && m.write, entry.execute && m.execute)
        } else {
            (entry.read, entry.write, entry.execute)
        };

        if req.want_read && !r { return false; }
        if req.want_write && !w { return false; }
        if req.want_execute && !x { return false; }
        true
    }
}

// ===== User Database =====

pub struct UserDatabase {
    users: [User; MAX_USERS],
    user_count: usize,
    groups: [Group; MAX_GROUPS],
    group_count: usize,
    next_uid: AtomicU32,
    next_gid: AtomicU32,
}

impl UserDatabase {
    pub fn new() -> Self {
        let mut db = UserDatabase {
            users: [User::new(0, 0); MAX_USERS],
            user_count: 0,
            groups: [Group::new(0); MAX_GROUPS],
            group_count: 0,
            next_uid: AtomicU32::new(1000),
            next_gid: AtomicU32::new(1000),
        };

        // Create root user
        db.create_system_user("root", UID_ROOT, GID_ROOT);
        
        // Create system users
        db.create_system_user("system", UID_SYSTEM, GID_ROOT);
        db.create_system_user("nobody", UID_NOBODY, GID_NOBODY);

        // Create system groups
        db.create_group("root", GID_ROOT, true);
        db.create_group("wheel", GID_WHEEL, true);
        db.create_group("users", GID_USERS, true);
        db.create_group("nobody", GID_NOBODY, true);

        db
    }

    fn create_system_user(&mut self, name: &str, uid: u32, gid: u32) {
        if self.user_count >= MAX_USERS {
            return;
        }
        let mut user = User::new(uid, gid);
        user.set_username(name);
        if uid == UID_ROOT {
            user.flags = UserFlags::root_account();
        } else {
            user.flags = UserFlags::system_account();
        }
        self.users[self.user_count] = user;
        self.user_count += 1;
    }

    pub fn create_user(&mut self, name: &str, gid: u32) -> Result<u32, &'static str> {
        if self.user_count >= MAX_USERS {
            return Err("User limit reached");
        }

        // Check for duplicate
        for i in 0..self.user_count {
            if self.users[i].username() == name {
                return Err("User already exists");
            }
        }

        let uid = self.next_uid.fetch_add(1, Ordering::SeqCst);
        let mut user = User::new(uid, gid);
        user.set_username(name);
        self.users[self.user_count] = user;
        self.user_count += 1;
        Ok(uid)
    }

    pub fn delete_user(&mut self, uid: u32) -> Result<(), &'static str> {
        if uid == UID_ROOT {
            return Err("Cannot delete root");
        }

        for i in 0..self.user_count {
            if self.users[i].uid == uid {
                for j in i..self.user_count - 1 {
                    self.users[j] = self.users[j + 1];
                }
                self.user_count -= 1;
                return Ok(());
            }
        }
        Err("User not found")
    }

    pub fn get_user(&self, uid: u32) -> Option<&User> {
        for i in 0..self.user_count {
            if self.users[i].uid == uid {
                return Some(&self.users[i]);
            }
        }
        None
    }

    pub fn get_user_mut(&mut self, uid: u32) -> Option<&mut User> {
        for i in 0..self.user_count {
            if self.users[i].uid == uid {
                return Some(&mut self.users[i]);
            }
        }
        None
    }

    pub fn find_user_by_name(&self, name: &str) -> Option<&User> {
        for i in 0..self.user_count {
            if self.users[i].username() == name {
                return Some(&self.users[i]);
            }
        }
        None
    }

    pub fn create_group(&mut self, name: &str, gid: u32, system: bool) -> bool {
        if self.group_count >= MAX_GROUPS {
            return false;
        }

        let mut group = Group::new(gid);
        group.set_name(name);
        group.system_group = system;
        self.groups[self.group_count] = group;
        self.group_count += 1;
        true
    }

    pub fn get_group(&self, gid: u32) -> Option<&Group> {
        for i in 0..self.group_count {
            if self.groups[i].gid == gid {
                return Some(&self.groups[i]);
            }
        }
        None
    }

    pub fn get_group_mut(&mut self, gid: u32) -> Option<&mut Group> {
        for i in 0..self.group_count {
            if self.groups[i].gid == gid {
                return Some(&mut self.groups[i]);
            }
        }
        None
    }

    pub fn user_count(&self) -> usize {
        self.user_count
    }

    pub fn group_count(&self) -> usize {
        self.group_count
    }
}

// ===== Session =====

#[derive(Copy, Clone)]
pub struct Session {
    /// Session ID
    pub session_id: u32,
    /// User ID
    pub uid: u32,
    /// Primary group ID
    pub gid: u32,
    /// Effective user ID
    pub euid: u32,
    /// Effective group ID
    pub egid: u32,
    /// Supplementary groups mask
    pub groups_mask: u64,
    /// Session start time
    pub start_time: u64,
    /// Is interactive session
    pub interactive: bool,
    /// TTY device (if any)
    pub tty: u32,
    /// Process group leader
    pub pgid: u32,
    /// Is session leader
    pub session_leader: bool,
}

impl Session {
    pub fn new(session_id: u32, uid: u32, gid: u32) -> Self {
        Session {
            session_id,
            uid,
            gid,
            euid: uid,
            egid: gid,
            groups_mask: 0,
            start_time: 0,
            interactive: true,
            tty: 0,
            pgid: 0,
            session_leader: true,
        }
    }

    pub fn make_request(&self, read: bool, write: bool, execute: bool) -> PermissionRequest {
        PermissionRequest {
            uid: self.euid,
            gid: self.egid,
            groups_mask: self.groups_mask,
            want_read: read,
            want_write: write,
            want_execute: execute,
        }
    }

    pub fn setuid(&mut self, uid: u32) -> bool {
        // Only root can setuid
        if self.euid != UID_ROOT && self.uid != UID_ROOT {
            return false;
        }
        self.euid = uid;
        true
    }

    pub fn setgid(&mut self, gid: u32) -> bool {
        if self.euid != UID_ROOT && self.uid != UID_ROOT {
            return false;
        }
        self.egid = gid;
        true
    }

    pub fn seteuid(&mut self, euid: u32) -> bool {
        if self.euid != UID_ROOT && self.uid != UID_ROOT && euid != self.uid {
            return false;
        }
        self.euid = euid;
        true
    }
}

// ===== Session Manager =====

pub struct SessionManager {
    sessions: [Session; MAX_SESSIONS],
    session_count: usize,
    next_session_id: AtomicU32,
}

impl SessionManager {
    pub fn new() -> Self {
        SessionManager {
            sessions: [Session::new(0, UID_NOBODY, GID_NOBODY); MAX_SESSIONS],
            session_count: 0,
            next_session_id: AtomicU32::new(1),
        }
    }

    pub fn create_session(&mut self, uid: u32, gid: u32) -> Result<u32, &'static str> {
        if self.session_count >= MAX_SESSIONS {
            return Err("Session limit reached");
        }

        let sid = self.next_session_id.fetch_add(1, Ordering::SeqCst);
        self.sessions[self.session_count] = Session::new(sid, uid, gid);
        self.session_count += 1;
        Ok(sid)
    }

    pub fn destroy_session(&mut self, session_id: u32) -> bool {
        for i in 0..self.session_count {
            if self.sessions[i].session_id == session_id {
                for j in i..self.session_count - 1 {
                    self.sessions[j] = self.sessions[j + 1];
                }
                self.session_count -= 1;
                return true;
            }
        }
        false
    }

    pub fn get_session(&self, session_id: u32) -> Option<&Session> {
        for i in 0..self.session_count {
            if self.sessions[i].session_id == session_id {
                return Some(&self.sessions[i]);
            }
        }
        None
    }

    pub fn get_session_mut(&mut self, session_id: u32) -> Option<&mut Session> {
        for i in 0..self.session_count {
            if self.sessions[i].session_id == session_id {
                return Some(&mut self.sessions[i]);
            }
        }
        None
    }

    pub fn session_count(&self) -> usize {
        self.session_count
    }
}

// ===== Tests =====

pub fn test_user_creation() -> bool {
    let mut db = UserDatabase::new();

    // Root should exist
    if db.get_user(UID_ROOT).is_none() {
        return false;
    }

    // Create user
    let uid = match db.create_user("testuser", GID_USERS) {
        Ok(u) => u,
        Err(_) => return false,
    };

    // Verify
    let user = match db.get_user(uid) {
        Some(u) => u,
        None => return false,
    };

    if user.username() != "testuser" {
        return false;
    }

    // Delete user
    if db.delete_user(uid).is_err() {
        return false;
    }

    true
}

pub fn test_permission_check() -> bool {
    let ownership = FileOwnership::new(1000, 100, FileMode::new(0o644));

    // Owner can read
    let req = PermissionRequest::read(1000, 100);
    if !ownership.check_permission(&req) {
        return false;
    }

    // Owner can write
    let req = PermissionRequest::write(1000, 100);
    if !ownership.check_permission(&req) {
        return false;
    }

    // Other can read
    let req = PermissionRequest::read(2000, 200);
    if !ownership.check_permission(&req) {
        return false;
    }

    // Other cannot write
    let req = PermissionRequest::write(2000, 200);
    if ownership.check_permission(&req) {
        return false;
    }

    // Root can do anything
    let req = PermissionRequest::write(UID_ROOT, GID_ROOT);
    if !ownership.check_permission(&req) {
        return false;
    }

    true
}

pub fn test_group_membership() -> bool {
    let mut user = User::new(1000, 100);
    user.set_username("testuser");

    // Add to groups
    if !user.add_group(10) { return false; }
    if !user.add_group(20) { return false; }

    // Check membership
    if !user.is_in_group(100) { return false; }  // Primary
    if !user.is_in_group(10) { return false; }   // Supplementary
    if !user.is_in_group(20) { return false; }   // Supplementary
    if user.is_in_group(30) { return false; }    // Not member

    // Remove from group
    if !user.remove_group(10) { return false; }
    if user.is_in_group(10) { return false; }

    true
}

pub fn test_acl() -> bool {
    let mut acl = Acl::new();

    // Add entries
    acl.add_entry(AclEntry::new(AclTag::UserObj, 0, true, true, true));
    acl.add_entry(AclEntry::new(AclTag::User, 1001, true, false, false));
    acl.add_entry(AclEntry::new(AclTag::GroupObj, 0, true, false, true));
    acl.add_entry(AclEntry::new(AclTag::Mask, 0, true, true, true));
    acl.add_entry(AclEntry::new(AclTag::Other, 0, true, false, false));

    // Find entries
    if acl.find_user(1001).is_none() { return false; }
    if acl.find_user(9999).is_some() { return false; }
    if acl.get_mask().is_none() { return false; }

    true
}

pub fn test_session() -> bool {
    let mut session = Session::new(1, 1000, 100);

    // Initial state
    if session.euid != 1000 { return false; }

    // Non-root cannot setuid to arbitrary user
    if session.setuid(2000) { return false; }

    // Make root
    session.euid = UID_ROOT;

    // Now can setuid
    if !session.setuid(2000) { return false; }
    if session.euid != 2000 { return false; }

    true
}
