use soroban_sdk::Env;

use crate::errors::ProgressError;
use crate::types::{DataKey, ProgressEntry};

/// Return the current configured history ring-buffer depth.
///
/// Falls back to [`crate::constants::DEFAULT_HISTORY_MAX_DEPTH`] when no
/// admin value has been written, preserving backwards-compatibility with
/// deployments that have never called `set_history_max_depth`.
pub fn get_history_max_depth(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::HistoryMaxDepth)
        .unwrap_or(crate::constants::DEFAULT_HISTORY_MAX_DEPTH)
}

/// Persist the admin-chosen ring-buffer depth.
///
/// Validation (non-zero, ≤ [`crate::constants::MAX_HISTORY_DEPTH`]) is
/// enforced by the public contract function before this is called.
pub fn set_history_max_depth(env: &Env, depth: u32) {
    env.storage()
        .instance()
        .set(&DataKey::HistoryMaxDepth, &depth);
}

/// Push a new history entry for `player_id`, evicting the oldest entry when
/// the ring is full.
///
/// The ring is implemented as a monotonically-increasing counter
/// (`HistoryCounter`) combined with per-index storage keys
/// (`HistoryEntry(player_id, index)`).  The "active window" is always the
/// most-recent `depth` indices.
///
/// **Lazy truncation on depth decrease**: when an admin reduces the depth
/// (e.g. from 20 → 5) entries outside the new window are **not** deleted
/// immediately.  Each subsequent write evicts the oldest out-of-window entry
/// until the ring converges to the new depth.
pub fn push_history_entry(
    env: &Env,
    player_id: u64,
    entry: &ProgressEntry,
) -> Result<(), ProgressError> {
    let history_key = DataKey::HistoryCounter(player_id);
    let index: u32 = env
        .storage()
        .persistent()
        .get(&history_key)
        .unwrap_or(0u32);
    let next_index = index
        .checked_add(1)
        .ok_or(ProgressError::Overflow)?;

    env.storage()
        .persistent()
        .set(&DataKey::HistoryEntry(player_id, next_index), entry);
    env.storage().persistent().set(&history_key, &next_index);

    // Evict the entry that falls outside the active window, if any.
    let depth = get_history_max_depth(env);
    if next_index > depth {
        let evict_index = next_index - depth;
        env.storage()
            .persistent()
            .remove(&DataKey::HistoryEntry(player_id, evict_index));
    }

    Ok(())
}
