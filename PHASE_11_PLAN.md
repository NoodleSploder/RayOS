# Phase 11: Hardware Integration & Advanced Features
## Planning Document (2026-01-07)

**Status:** PLANNING  
**Estimated Tasks:** 5-6 major features  
**Estimated Lines:** 4,000-5,000+  
**Target Completion:** This session  

---

## Phase 11 Vision

Phase 10 established the **architectural foundation** (security, networking, observability). Phase 11 will focus on **hardware integration** and **production hardening**:

1. **Device Handler Wiring** - Connect policy checks to actual virtio device operations
2. **DHCP & Network Stack** - Full network initialization and connectivity
3. **TPM 2.0 Integration** - Real measured boot with PCR measurements
4. **Performance Optimization** - Firewall hash tables, metrics ring buffers
5. **Advanced Security Features** - Temporary capabilities, delegation, fine-grained controls
6. **Scalability Layer** - Support 32+ VMs, hierarchical policies

---

## Task Breakdown

### Task 1: Virtio Device Handler Integration (Est. 500-700 lines)
**Goal:** Wire policy enforcement checks into actual device operations

**Deliverables:**
- `src/device_handlers.rs` (NEW - 500+ lines)
  * VirtioGpuHandler with policy checks
  * VirtioNetHandler with firewall integration
  * VirtioBlkHandler with disk quota enforcement
  * VirtioInputHandler with capability checks
  * VirtioConsoleHandler with audit logging

- Enhanced `src/shell.rs` (+200 lines)
  * `device` command: status, handlers, list
  * Per-device policy overrides
  * Handler statistics (ops, denials, latency)

**Features:**
- Pre-operation policy verification
- Latency-aware enforcement (skip checks for trusted paths)
- Per-handler audit events
- Device-specific quotas (GPU memory, network bandwidth)
- Hot-plug device capability grants

**Example Flow:**
```
VM calls GPU operation
  → Check capability (GPU in capabilities?)
  → Get device handler (VirtioGpuHandler)
  → Pre-op checks (memory quota, priority)
  → Execute operation
  → Post-op audit log
  → Update metrics
```

---

### Task 2: DHCP Client & Network Initialization (Est. 600-800 lines)
**Goal:** Implement real DHCP discovery and network connectivity

**Deliverables:**
- `src/dhcp.rs` (NEW - 600+ lines)
  * DhcpClient struct with state machine
  * Packet crafting (DISCOVER, REQUEST, ACKNOWLEDGE)
  * Lease management and renewal
  * DNS configuration
  * NTP time synchronization

- Enhanced `src/firewall.rs` (+150 lines)
  * Allow DHCP traffic (UDP 67/68) by default
  * DHCP server spoofing protection
  * Configurable DHCP servers (default 8.8.8.8)

- Enhanced `src/shell.rs` (+200 lines)
  * `dhcp` command: status, renew, release, logs
  * DNS resolver configuration
  * NTP time service
  * Network diagnostic tools

**Features:**
- RFC 2131-compliant DHCP client
- Lease tracking with auto-renewal
- Multiple DHCP server fallback
- Static IP fallback
- ARP probe before accepting lease
- DHCP transaction logging

**State Machine:**
```
INIT → SELECTING → REQUESTING → BOUND ↔ RENEWING → REBINDING → INIT (on failure)
```

---

### Task 3: TPM 2.0 Measured Boot Integration (Est. 700-900 lines)
**Goal:** Enable real TPM 2.0 PCR measurements during boot and runtime

**Deliverables:**
- `src/tpm2.rs` (NEW - 700+ lines)
  * Tpm2Device for TPM 2.0 command interface
  * PCRExtend operations with SHA256
  * PCR read and attestation
  * NV (Non-Volatile) storage for policies
  * Event log tracking

- Enhanced `src/security.rs` (+250 lines)
  * Real PCR measurement during boot
  * Kernel/initrd measurement
  * Policy measurement
  * VM launch PCR updates
  * Attestation quote generation

- Enhanced `src/shell.rs` (+200 lines)
  * `tpm` command: status, pcr, extend, quote, nv
  * Measured boot timeline
  * PCR history and changes
  * Attestation verification

**Features:**
- SHA256 PCR bank (PCR[0-7])
- Boot event log with hashes
- Kernel + initrd + policy measurements
- Per-VM PCR snapshots
- TPM2_Quote generation for remote attestation
- NV storage for policy seeds

**PCR Layout:**
```
PCR[0]: UEFI firmware + bootloader
PCR[4]: Boot config + kernel command line
PCR[5]: Bootloader actions
PCR[7]: Secure Boot policy
PCR[8]: RayOS kernel image
PCR[9]: RayOS policies
PCR[10]: VM launch events
```

---

### Task 4: Performance Optimization (Est. 400-600 lines)
**Goal:** Optimize hot paths for <1us latency

**Deliverables:**
- `src/performance.rs` (NEW - 400+ lines)
  * FirewallHashTable (replace linear scan)
  * MetricsRingBuffer (zero-copy export)
  * CapabilityCache (per-VM capability lookups)
  * Fast path optimizations

- Enhanced `src/firewall.rs` (+150 lines)
  * Hash table implementation for rules
  * Rule priority indexing
  * Early-exit optimization

- Enhanced `src/observability.rs` (+150 lines)
  * Ring buffer metrics collection
  * Lock-free statistics counters
  * Batch export optimization

- Enhanced `src/shell.rs` (+150 lines)
  * `perf` command: bench, profile-internal, optimize
  * Detailed latency breakdowns
  * Cache hit/miss rates

**Performance Targets:**
- Policy check: <100ns (currently ~100ns)
- Firewall match: <1µs (currently <10ms)
- Capability cache hit: <50ns
- Metrics write: <100ns (currently <200ns)

**Optimizations:**
```
Firewall:
  Hash table: rule_id % 64 → O(1) lookup
  Cache: per-VM rule matching results (TTL 10ms)

Capabilities:
  Bitmask cache in VM context (TLB-friendly)
  Lazy invalidation on changes

Metrics:
  Lock-free ring buffer
  Per-CPU counters
  Batch timestamp collection
```

---

### Task 5: Advanced Security Features (Est. 500-700 lines)
**Goal:** Fine-grained capability control and runtime policy updates

**Deliverables:**
- Enhanced `src/security.rs` (+300 lines)
  * TemporaryCapability with expiration timestamps
  * CapabilityDelegation (VM can grant caps to sub-VMs)
  * FineGrainedCapability (port ranges, bandwidth limits)
  * PolicyUpdate mechanism (hot policy swaps)

- Enhanced `src/policy_enforcement.rs` (+250 lines)
  * DynamicPolicyEnforcer with on-the-fly updates
  * CapabilityExpiration checking
  * Bandwidth rate limiting
  * Port-range restrictions
  * Delegation chains (max depth 3)

- Enhanced `src/shell.rs` (+200 lines)
  * `cap` command: grant-temp, delegate, fine-grain, revoke-all
  * Delegation tree visualization
  * Bandwidth QoS configuration
  * Port access policies

**Features:**
- Temporary capabilities (1 hour default, configurable)
- Hierarchical delegation (depth tracking)
- Per-port firewall rules (e.g., allow TCP 443 only)
- Bandwidth quotas per VM (e.g., max 100 Mbps)
- Automatic revocation on expiration
- Audit trail for all policy changes

**Example Usage:**
```
# Grant temporary network access to VM 1000 for 30 minutes
cap grant-temp 1000 CAP_NETWORK --expires 30m

# Delegate capability (VM 1000 grants CAP_DISK_READ to 1001)
cap delegate 1000 1001 CAP_DISK_READ

# Fine-grained network control
cap fine-grain 1000 CAP_NETWORK --allow-ports 443,80 --max-bandwidth 50Mbps

# Revoke all temporary capabilities
cap revoke-all --temporary
```

---

### Task 6: Scalability Layer (Est. 600-800 lines)
**Goal:** Support 32+ VMs with distributed policy enforcement

**Deliverables:**
- `src/scalability.rs` (NEW - 600+ lines)
  * HierarchicalPolicyEngine (parent/child VM policies)
  * PolicyDistribution (broadcast updates to 32+ VMs)
  * LoadBalancedFirewall (multi-CPU firewall processing)
  * VM grouping and policies

- Enhanced `src/policy_enforcement.rs` (+150 lines)
  * Policy inheritance from parent VMs
  * Group-based policies
  * Quota distribution across groups

- Enhanced `src/shell.rs` (+250 lines)
  * `policy` command: group, distribute, load-balance
  * VM tree visualization
  * Multi-VM commands (apply policy to 10+ VMs at once)
  * Scalability metrics (latency per 8/16/32/64 VMs)

**Features:**
- Support 64 VMs (currently 8)
- Parent-child VM relationships (container-like)
- Shared policy inheritance
- Group-based capabilities
- Multi-CPU policy enforcement
- Automatic load balancing

**Architecture:**
```
Root Policy Engine
  ├─ Group 1 (Linux Desktop VMs 1000-1007)
  │   ├─ VM 1000: parent capabilities + overrides
  │   ├─ VM 1001: parent capabilities + overrides
  │   └─ ...
  ├─ Group 2 (Server VMs 2000-2007)
  │   ├─ VM 2000: parent capabilities + overrides
  │   └─ ...
  └─ Group 3 (Restricted VMs 3000-3015)
      └─ ...
```

---

## Implementation Strategy

### Priority Order
1. **Task 1: Device Handlers** (CRITICAL - unblocks everything)
2. **Task 3: TPM 2.0** (HIGH - security foundation)
3. **Task 2: DHCP** (HIGH - network initialization)
4. **Task 4: Performance** (MEDIUM - optimization)
5. **Task 5: Advanced Security** (MEDIUM - feature richness)
6. **Task 6: Scalability** (LOW - future-proofing)

### Estimated Effort
| Task | Lines | Build Time | Complexity | Priority |
|------|-------|-----------|-----------|----------|
| Device Handlers | 700 | +0.5s | High | 1 |
| TPM 2.0 | 950 | +0.6s | High | 2 |
| DHCP | 800 | +0.5s | High | 3 |
| Performance | 550 | +0.4s | Medium | 4 |
| Advanced Security | 550 | +0.4s | Medium | 5 |
| Scalability | 750 | +0.5s | Medium | 6 |
| **TOTAL** | **4,300+** | **+3.0s** | - | - |

**Target Build Time After Phase 11:** ~13.5s (vs 10.51s currently)

---

## Success Criteria

### Task 1: Device Handlers
- ✅ All 4 device types integrated with policy checks
- ✅ <1% CPU overhead per policy check
- ✅ 100+ device operations logged per second

### Task 2: DHCP
- ✅ Automatic IP assignment from DHCP server
- ✅ Lease renewal every 12-24 hours
- ✅ ARP conflict detection
- ✅ Fallback to static IP

### Task 3: TPM 2.0
- ✅ Real SHA256 PCR measurements
- ✅ Boot event log with 100+ entries
- ✅ VM launch events recorded
- ✅ Attestation quotes generate correctly

### Task 4: Performance
- ✅ Firewall matches in <1µs (10x improvement)
- ✅ Policy checks <100ns (no regression)
- ✅ 0 build warnings added

### Task 5: Advanced Security
- ✅ Temporary capabilities auto-revoke on expiration
- ✅ Bandwidth rate limiting enforced
- ✅ Port-range restrictions working
- ✅ Delegation chains tracked

### Task 6: Scalability
- ✅ Support 32 concurrent VMs
- ✅ Policy updates distributed in <10ms
- ✅ Load balanced across CPUs
- ✅ <1% latency increase at 32 VMs

---

## Next Steps

**Ready to begin Phase 11 Task 1: Device Handler Integration**

Execute:
```bash
cd /home/noodlesploder/repos/RayOS/crates/kernel-bare
cargo +nightly build --release --target x86_64-unknown-none
```

Starting with `src/device_handlers.rs` (~500 lines of virtio integration).

---

End of Phase 11 Planning
