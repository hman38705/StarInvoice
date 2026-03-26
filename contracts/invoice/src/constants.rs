/// Storage TTL (Time-To-Live) constants for Soroban persistent storage.
/// 
/// Soroban persistent storage entries expire unless their TTL is extended.
/// These constants define the TTL extension parameters used throughout the contract.

/// Minimum TTL threshold before extending (in ledgers).
/// If an entry's TTL is below this threshold, it will be extended.
pub const TTL_THRESHOLD: u32 = 518400; // ~30 days (assuming 5-second ledgers)

/// TTL extension duration (in ledgers).
/// When extending TTL, entries will be extended to this duration.
pub const TTL_EXTEND_TO: u32 = 1036800; // ~60 days (assuming 5-second ledgers)