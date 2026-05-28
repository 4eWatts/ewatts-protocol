pub const TARGET_BLOCK_TIME_SECS: u64 = 600;
pub const BLOCKS_PER_DAY: u64 = 144;
pub const BLOCKS_PER_YEAR: u64 = 52596;
pub const DAG_INITIAL_SIZE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
pub const DAG_ELEMENT_SIZE: usize = 64;
pub const DAG_GROWTH_RATE_BYTES_PER_YEAR: u64 = 512 * 1024 * 1024;
pub const DAG_EPOCH_BLOCKS: u64 = 2016;
pub const DAG_INITIAL_CACHE_RATIO: u64 = 128;
pub const DAG_MIX_ROUNDS: u32 = 256;
pub const DAG_ACCELERATION_RATE: u64 = 1024 * 1024 * 1024;
pub const DAG_ACCELERATION_THRESHOLD_ETA: f64 = 1.3;
pub const DAG_ACCELERATION_THRESHOLD_BANDWIDTH: f64 = 100.0;
pub const DAG_ACCELERATION_YEARS: u32 = 2;
pub const DECIMAL_PLACES: u32 = 6;
pub const UNITS_PER_EWATT: u64 = 1_000_000;

// ─── Fixed-point precision constants (f64→u64 migration) ──────────────
pub const EMISSION_PRECISION: u64 = 1_000_000_000;        // 1e9 for emission rates
pub const COMMIT_PRECISION: u64 = 1_000_000_000;          // 1e9 for effective commit
pub const VR_PRECISION: u64 = 1_000_000;                  // 1e6 for VR (kWh/Ewatt)
pub const EFF_PRECISION: u64 = 1_000_000;                 // 1e6 for efficiency (0-1)
pub const CAP_PRECISION: u64 = 1_000_000;                 // 1e6 for ramp-up cap (0-1)
pub const RATE_PRECISION: u64 = 1_000_000;                // 1e6 for rate ratios

// ─── Supply-Based Emission ─────────────────────────────────────────
/// Annual supply growth rate: 1.5% nominal (asymptotic minimum).
/// The growth rate converges to 1.5%/year as the network grows large.
/// In early stages with few miners, emission is higher:
///   R = supply × (0.015 / 52596) × (REF_COMMIT + te) / te
/// At 1 miner: 3.0%/year. At 3 miners: 2.0%/year. At infinity: 1.5%/year.
///
/// "Inflation" in eWatts is DRAM efficiency improvement (~1.5%/year), not
/// consumer price inflation. The asymptotic rate matches the drift of the
/// physical anchor: 1.5% nominal ≈ 0% real dilution in the limit.
pub const ANNUAL_GROWTH_RATE: u64 = 15_000;  // 1.5% in RATE_PRECISION (1e6)

/// Reference commitment: one standard miner at 1 GB/s with full efficiency.
/// Used in the emission formula as the bootstrap inverse multiplier:
///   multiplier = (REF_COMMIT + te) / te
/// At 1 miner: 2×. At 10 miners: 1.1×. At infinity: 1×.
pub const REF_COMMIT_INT: u64 = 1_000_000_000;  // ~1 GB/s miner in COMMIT_PRECISION (1e9)

// ─── Legacy constants (kept for genesis supply and safety caps) ────
pub const RAMP_UP_CAP_INT: u64 = 800_000;                  // 0.80 * 1e6
pub const MIN_COMMIT_GBS_INT: u64 = 1_000_000_000;         // 1.0 GB/s in milli-GB/s
pub const EFFICIENCY_PENALTY_THRESHOLD_INT: u64 = 700_000; // 0.7 * 1e6
pub const EFFICIENCY_CAP_THRESHOLD_INT: u64 = 1_300_000;   // 1.3 * 1e6
pub const TESTNET_RAMP_UP_CAP_INT: u64 = 800_000;          // 0.80 * 1e6
pub const BASE_EMISSION_UNITS: u64 = 100_000_000;   // 100 Ewatt em base units (genesis supply)
pub const COMMIT_WINDOW_BLOCKS: u64 = 4300;           // v27: 30 days (was 1000 = 7 days)
pub const RAMP_UP_BLOCKS: u64 = 10000;
pub const RAMP_UP_CAP: f64 = 0.80;                    // v27: max 80% reward per miner during ramp-up
pub const FOUNDER_LOCK_BLOCKS: u64 = 50000;            // v27: founder cannot spend until block 50000
pub const FOUNDER_LOCK_ADDITIONAL: u64 = 40000;        // v27: additional lock blocks after coinbase
pub const MIN_COMMIT_GBS: f64 = 1.0;
pub const EFFICIENCY_PENALTY_THRESHOLD: f64 = 0.7;
pub const EFFICIENCY_CAP_THRESHOLD: f64 = 1.3;
pub const BASE_ACCESSES: u64 = 1_000_000_000;
pub const VERIFICATION_SAMPLE_RATE: f64 = 0.001;
pub const DIFFICULTY_WINDOW_BLOCKS: u64 = 100;
pub const DIFFICULTY_BOUND_MIN: f64 = 0.5;
pub const DIFFICULTY_BOUND_MAX: f64 = 2.0;
pub const VR_WINDOW_BLOCKS: u64 = 1000;
pub const J_PER_GB: f64 = 0.08;
pub const J_PER_KWH: f64 = 3_600_000.0;
pub const RING_SIGNATURE_SIZE: usize = 11;
pub const MAX_BLOCK_TXS: usize = 10000;
pub const TESTNET_DAG_SIZE: u64 = 4 * 1024 * 1024;
pub const TESTNET_BLOCK_TIME: u64 = 60;
pub const TESTNET_RAMP_UP: u64 = 100;
pub const TESTNET_COMMIT_WINDOW: u64 = 43;   // v27: 30 day equiv on testnet
pub const TESTNET_RAMP_UP_CAP: f64 = 0.80;
pub const TESTNET_FOUNDER_LOCK: u64 = 500;
pub const MAX_PEERS: usize = 125;
pub const PROTOCOL_VERSION: u32 = 0x0003;

// ─── P2PKH & Quantum Migration ─────────────────────────────────────
/// P2PKH hash output size (SHA256 truncated to 20 bytes).
pub const PUBKEY_HASH_SIZE: usize = 20;

/// Block height at which quantum migration activates (~10 years).
pub const QUANTUM_ACTIVATION_BLOCK: u64 = 3_153_600;

/// Supermajority of miners required to activate PQ signature scheme.
pub const PQ_MINER_SUPERMAJORITY: f64 = 0.90;

/// Post-quantum signature scheme placeholder: FALCON-1024.
/// Migration plan: hard fork at QUANTUM_ACTIVATION_BLOCK requires 90% miner consensus.
pub const PQ_SIG_SCHEME: &str = "FALCON-1024";
