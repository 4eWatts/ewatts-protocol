// ─── Block Timing ─────────────────────────────────────────────────────
pub const TARGET_BLOCK_TIME_SECS: u64 = 600;
pub const BLOCKS_PER_DAY: u64 = 144;
pub const BLOCKS_PER_YEAR: u64 = 52560;  // 365d × 144 (CORRECTED v3)
pub const VR_WINDOW_BLOCKS: u64 = 1000;

// ─── eWatts v3 Protocol Parameters ────────────────────────────────────
// (from whitepaper v28 — neutral settlement layer anchored to memory-bound computation)

/// Bootstrap multiplier at block 1: M = 100,000 × exp(-k × S / S_threshold)
pub const M_MAX: u64 = 100_000;

/// Maturity threshold: 10 billion eWatts in base units.
/// S_THRESHOLD_UNITS = 10,000,000,000 × 1,000,000 = 10^16
pub const S_THRESHOLD_UNITS: u64 = 10_000_000_000 * UNITS_PER_EWATT;

/// Reference network size at which cost = P_target at maturity.
pub const N_CALIBRATION: u64 = 100_000;

/// Target production cost per eWatt at maturity (in micro-USD).
/// P_target = $1.00 = 1,000,000 micro-USD
pub const P_TARGET_MICRO: u64 = 1_000_000;

/// ln(M_MAX) × 1,000,000 for fixed-point exponential computation.
/// ln(100,000) ≈ 11.512925 → multiplied by 1,000,000 = 11,512,925
pub const LN_M_MAX_PRECISION: u64 = 11_512_925;

// ─── DAG (Proof-of-Work) ─────────────────────────────────────────────
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

// ─── Precision / Units ───────────────────────────────────────────────
pub const DECIMAL_PLACES: u32 = 6;
pub const UNITS_PER_EWATT: u64 = 1_000_000;  // 1 eWatt = 1,000,000 base units

pub const EMISSION_PRECISION: u64 = 1_000_000_000;        // 1e9 for emission rates
pub const COMMIT_PRECISION: u64 = 1_000_000_000;          // 1e9 for effective commit
pub const VR_PRECISION: u64 = 1_000_000;                  // 1e6 for VR (kWh/Ewatt)
pub const EFF_PRECISION: u64 = 1_000_000;                 // 1e6 for efficiency (0-1)
pub const CAP_PRECISION: u64 = 1_000_000;                 // 1e6 for ramp-up cap (0-1)
pub const RATE_PRECISION: u64 = 1_000_000;                // 1e6 for rate ratios

// ─── v27 Emission (DEPRECATED — kept for migration compatibility) ─────
pub const BASE_EMISSION_INT: u64 = 100_000_000 * EMISSION_PRECISION / UNITS_PER_EWATT;
pub const EFF_REF_INT: u64 = 1_000_000;
pub const EMISSION_FLOOR_MULTIPLIER_INT: u64 = 50_000_000;
pub const EMISSION_CEILING_MULTIPLIER_INT: u64 = 20_000_000_000_000;
pub const RAMP_UP_CAP_INT: u64 = 800_000;
pub const MIN_COMMIT_GBS_INT: u64 = 1_000_000_000;
pub const EFFICIENCY_PENALTY_THRESHOLD_INT: u64 = 700_000;
pub const EFFICIENCY_CAP_THRESHOLD_INT: u64 = 1_300_000;
pub const TESTNET_RAMP_UP_CAP_INT: u64 = 800_000;

pub const BASE_EMISSION: f64 = 100.0;
pub const BASE_EMISSION_UNITS: u64 = 100_000_000;
pub const EMISSION_FLOOR_MULTIPLIER: f64 = 0.05;
pub const EMISSION_CEILING_MULTIPLIER: f64 = 20.0;
pub const COMMIT_WINDOW_BLOCKS: u64 = 4300;
pub const RAMP_UP_BLOCKS: u64 = 10000;
pub const RAMP_UP_CAP: f64 = 0.80;
pub const FOUNDER_LOCK_BLOCKS: u64 = 50000;
pub const FOUNDER_LOCK_ADDITIONAL: u64 = 40000;
pub const MIN_COMMIT_GBS: f64 = 1.0;
pub const EFFICIENCY_PENALTY_THRESHOLD: f64 = 0.7;
pub const EFFICIENCY_CAP_THRESHOLD: f64 = 1.3;

// ─── Mining & Proof ──────────────────────────────────────────────────
pub const BASE_ACCESSES: u64 = 1_000_000_000;
pub const VERIFICATION_SAMPLE_RATE: f64 = 0.001;

// ─── Difficulty ──────────────────────────────────────────────────────
pub const DIFFICULTY_WINDOW_BLOCKS: u64 = 100;
pub const DIFFICULTY_BOUND_MIN: f64 = 0.5;
pub const DIFFICULTY_BOUND_MAX: f64 = 2.0;

// ─── VR (Valor de Referência) Calibration ────────────────────────────
/// Energy per GB of memory access — used to compute on-chain VR.
///
/// v3: Calibrated to reflect TOTAL node power (~75W), not just memory bandwidth (~10W).
/// Previous value (0.08 J/GB) only covered DRAM transfer energy.
/// Updated value includes CPU, DRAM refresh, chipset idle, and power supply overhead.
///
/// TODO: Replace with empirical measurement from wattmeter on reference mining node.
/// Provisional value: ~6 J/GB (75W node at ~12.5 GB/s effective bandwidth).
pub const J_PER_GB: f64 = 6.0;
pub const J_PER_KWH: f64 = 3_600_000.0;

// ─── Ring Signatures (Privacy) ───────────────────────────────────────
pub const RING_SIGNATURE_SIZE: usize = 11;

// ─── Block / Mempool ─────────────────────────────────────────────────
pub const MAX_BLOCK_TXS: usize = 10000;

// ─── Testnet ─────────────────────────────────────────────────────────
pub const TESTNET_DAG_SIZE: u64 = 4 * 1024 * 1024;
pub const TESTNET_BLOCK_TIME: u64 = 60;
pub const TESTNET_RAMP_UP: u64 = 100;
pub const TESTNET_COMMIT_WINDOW: u64 = 43;
pub const TESTNET_RAMP_UP_CAP: f64 = 0.80;
pub const TESTNET_FOUNDER_LOCK: u64 = 500;

// ─── P2P ─────────────────────────────────────────────────────────────
pub const MAX_PEERS: usize = 125;
pub const PROTOCOL_VERSION: u32 = 0x0004;  // v3 emission formula

// ─── P2PKH & Quantum Migration ───────────────────────────────────────
pub const PUBKEY_HASH_SIZE: usize = 20;
pub const QUANTUM_ACTIVATION_BLOCK: u64 = 3_153_600;
pub const PQ_MINER_SUPERMAJORITY: f64 = 0.90;
pub const PQ_SIG_SCHEME: &str = "FALCON-1024";
