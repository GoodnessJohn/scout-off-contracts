#![no_std]
mod constants;
mod errors;
mod events;
mod storage;
mod types;

use constants::MAX_HISTORY_DEPTH;
use errors::ProgressError;
use types::{ContractHealth, DataKey, ProgressEntry, ProgressLevel};

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

const INSTANCE_TTL_MIN: u32 = 100;
const INSTANCE_TTL_MAX: u32 = 500;

const PERSISTENT_TTL_MIN: u32 = 500;
const PERSISTENT_TTL_MAX: u32 = 2000;

#[contract]
pub struct ProgressContract;

#[contractimpl]
impl ProgressContract {
    // ---------------------------------------------------------------------------
    // Admin
    // ---------------------------------------------------------------------------

    pub fn initialize(env: Env, admin: Address) -> Result<(), ProgressError> {
        Self::bump_instance_ttl(&env);
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(ProgressError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    pub fn pause_contract(env: Env) -> Result<(), ProgressError> {
        Self::bump_instance_ttl(&env);
        Self::require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    pub fn unpause_contract(env: Env) -> Result<(), ProgressError> {
        Self::bump_instance_ttl(&env);
        Self::require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    /// Transfer admin rights to a new address (current admin auth required).
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), ProgressError> {
        Self::bump_instance_ttl(&env);
        let old_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ProgressError::NotInitialized)?;
        old_admin.require_auth();
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        events::admin_transferred(&env, &old_admin, &new_admin);
        Ok(())
    }

    /// Reset a player's level for dispute resolution.
    /// Existing history is preserved; a new history entry records the reset.
    pub fn reset_player_level(
        env: Env,
        player_id: u64,
        target_level: ProgressLevel,
    ) -> Result<(), ProgressError> {
        Self::require_not_paused(&env)?;
        let admin = Self::require_admin(&env)?;

        let old_level = Self::get_current_level(&env, player_id);
        Self::record_progress_entry(
            &env,
            player_id,
            old_level.clone(),
            target_level.clone(),
            admin,
            0,
        )?;
        env.storage()
            .persistent()
            .set(&DataKey::PlayerLevel(player_id), &target_level);

        events::player_level_reset(&env, player_id, &old_level, &target_level);
        Ok(())
    }

    /// Set the history ring-buffer depth used for all players.
    ///
    /// Requires admin authentication.  Rejects `depth == 0` and
    /// `depth > MAX_HISTORY_DEPTH` with [`ProgressError::InvalidHistoryDepth`].
    ///
    /// # Lazy truncation on depth decrease
    ///
    /// When the depth is **reduced** (e.g. from 20 → 5), entries that now fall
    /// outside the active window are **not retroactively deleted**.  They remain
    /// in persistent storage until the next `advance_level` or
    /// `reset_player_level` call for that player triggers the eviction loop in
    /// [`storage::push_history_entry`].  Subsequent writes will converge the
    /// ring to the new depth one entry at a time.  This is intentional: a
    /// retroactive bulk-delete would require a variable-length loop whose gas
    /// cost is unbounded for large histories.
    pub fn set_history_max_depth(env: Env, depth: u32) -> Result<(), ProgressError> {
        Self::bump_instance_ttl(&env);
        Self::require_admin(&env)?;
        if depth == 0 || depth > MAX_HISTORY_DEPTH {
            return Err(ProgressError::InvalidHistoryDepth);
        }
        storage::set_history_max_depth(&env, depth);
        Ok(())
    }

    /// Return the currently configured history ring-buffer depth.
    ///
    /// Returns [`DEFAULT_HISTORY_MAX_DEPTH`] when no admin value has been set.
    pub fn get_history_max_depth(env: Env) -> u32 {
        storage::get_history_max_depth(&env)
    }

    // ---------------------------------------------------------------------------
    // Progress updates
    // ---------------------------------------------------------------------------

    /// Advance a player's progress level by one tier.
    /// Caller must be an authorized validator (or scout for Level 3).
    /// `milestone_ref` links back to the verification contract's milestone index.
    pub fn advance_level(
        env: Env,
        caller: Address,
        player_id: u64,
        milestone_ref: u32,
    ) -> Result<ProgressLevel, ProgressError> {
        Self::bump_instance_ttl(&env);
        Self::require_not_paused(&env)?;
        Self::require_initialized(&env)?;

        if let Some(verification_contract) = env
            .storage()
            .instance()
            .get::<DataKey, Address>(&DataKey::VerificationContract)
        {
            verification_contract.require_auth();
        } else {
            caller.require_auth();
        }

        let current = Self::get_current_level(&env, player_id);
        let new_level = current.next().ok_or(ProgressError::AlreadyAtMaxLevel)?;

        Self::record_progress_entry(
            &env,
            player_id,
            current.clone(),
            new_level.clone(),
            caller.clone(),
            milestone_ref,
        )?;
        env.storage()
            .persistent()
            .set(&DataKey::PlayerLevel(player_id), &new_level);

        events::progress_updated(
            &env,
            player_id,
            &current,
            &new_level,
            &caller,
            milestone_ref,
        );
        Ok(new_level)
    }

    // ---------------------------------------------------------------------------
    // Queries
    // ---------------------------------------------------------------------------

    pub fn get_level(env: Env, player_id: u64) -> ProgressLevel {
        env.storage()
            .persistent()
            .get(&DataKey::PlayerLevel(player_id))
            .unwrap_or(ProgressLevel::Unverified)
    }

    pub fn get_history_count(env: Env, player_id: u64) -> u32 {
        Self::bump_instance_ttl(&env);
        env.storage()
            .persistent()
            .get(&DataKey::HistoryCounter(player_id))
            .unwrap_or(0u32)
    }

    pub fn get_history_entry(
        env: Env,
        player_id: u64,
        index: u32,
    ) -> Result<ProgressEntry, ProgressError> {
        Self::bump_instance_ttl(&env);
        let entry: ProgressEntry = env
            .storage()
            .persistent()
            .get(&DataKey::HistoryEntry(player_id, index))
            .ok_or(ProgressError::PlayerNotFound)?;
        env.storage().persistent().extend_ttl(
            &DataKey::HistoryEntry(player_id, index),
            PERSISTENT_TTL_MIN,
            PERSISTENT_TTL_MAX,
        );
        Ok(entry)
    }

    /// Return all history entries for a player in chronological order (index 1..=N).
    /// Capped at 50 entries to bound gas consumption.
    /// Returns an empty Vec if the player has no history.
    pub fn get_progress_history(env: Env, player_id: u64) -> Vec<ProgressEntry> {
        const MAX_ENTRIES: u32 = 50;

        let count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::HistoryCounter(player_id))
            .unwrap_or(0u32);

        let limit = count.min(MAX_ENTRIES);
        let mut entries: Vec<ProgressEntry> = Vec::new(&env);

        for i in 1..=limit {
            if let Some(entry) = env
                .storage()
                .persistent()
                .get(&DataKey::HistoryEntry(player_id, i))
            {
                entries.push_back(entry);
            }
        }

        entries
    }

    pub fn health(env: Env) -> ContractHealth {
        Self::bump_instance_ttl(&env);
        let initialized = env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Initialized)
            .unwrap_or(false);
        let paused = env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false);
        ContractHealth {
            initialized,
            paused,
        }
    }

    // ---------------------------------------------------------------------------
    // Internal helpers
    // ---------------------------------------------------------------------------

    fn bump_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_TTL_MIN, INSTANCE_TTL_MAX);
    }

    fn get_current_level(env: &Env, player_id: u64) -> ProgressLevel {
        env.storage()
            .persistent()
            .get(&DataKey::PlayerLevel(player_id))
            .unwrap_or(ProgressLevel::Unverified)
    }

    fn record_progress_entry(
        env: &Env,
        player_id: u64,
        old_level: ProgressLevel,
        new_level: ProgressLevel,
        updated_by: Address,
        milestone_ref: u32,
    ) -> Result<(), ProgressError> {
        let entry = ProgressEntry {
            player_id,
            old_level,
            new_level,
            updated_by,
            updated_at: env.ledger().timestamp(),
            milestone_ref,
            ledger_sequence: env.ledger().sequence(),
        };
        storage::push_history_entry(env, player_id, &entry)
    }

    fn require_initialized(env: &Env) -> Result<(), ProgressError> {
        if !env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Initialized)
            .unwrap_or(false)
        {
            return Err(ProgressError::NotInitialized);
        }
        Ok(())
    }

    fn require_not_paused(env: &Env) -> Result<(), ProgressError> {
        if env
            .storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(ProgressError::ContractPaused);
        }
        Ok(())
    }

    fn require_admin(env: &Env) -> Result<Address, ProgressError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ProgressError::NotInitialized)?;
        admin.require_auth();
        Ok(admin)
    }
}

// =============================================================================
// Tests
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::constants::DEFAULT_HISTORY_MAX_DEPTH;
    use soroban_sdk::{
        testutils::{Address as _, Events as _},
        vec, Env, IntoVal, Symbol,
    };

    #[allow(deprecated)]
    fn setup() -> (Env, ProgressContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register_contract(None, ProgressContract);
        let client = ProgressContractClient::new(&env, &id);
        (env, client)
    }

    #[test]
    fn test_two_players_advance_independently() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);
        let validator = Address::generate(&env);

        client.advance_level(&validator, &1u64, &1u32);
        client.advance_level(&validator, &1u64, &2u32);
        client.advance_level(&validator, &2u64, &3u32);

        assert_eq!(client.get_level(&1u64), ProgressLevel::PerformanceMilestones);
        assert_eq!(client.get_level(&2u64), ProgressLevel::VerifiedIdentity);
        assert_eq!(client.get_history_count(&1u64), 2);
        assert_eq!(client.get_history_count(&2u64), 1);
    }

    #[test]
    fn test_advance_level_sequence() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 1u64;

        let l1 = client.advance_level(&validator, &player_id, &1u32);
        assert_eq!(l1, ProgressLevel::VerifiedIdentity);

        let l2 = client.advance_level(&validator, &player_id, &2u32);
        assert_eq!(l2, ProgressLevel::PerformanceMilestones);

        let l3 = client.advance_level(&validator, &player_id, &3u32);
        assert_eq!(l3, ProgressLevel::EliteTier);

        assert_eq!(client.get_history_count(&player_id), 3);
    }

    #[test]
    fn test_get_history_entry_correct_data() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 42u64;
        let milestone = 7u32;

        client.advance_level(&validator, &player_id, &milestone);

        let entry = client.get_history_entry(&player_id, &1u32);
        assert_eq!(entry.old_level, ProgressLevel::Unverified);
        assert_eq!(entry.new_level, ProgressLevel::VerifiedIdentity);
        assert_eq!(entry.updated_by, validator);
        assert_eq!(entry.milestone_ref, milestone);
    }

    #[test]
    #[allow(deprecated)]
    fn test_advance_level_not_initialized() {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register_contract(None, ProgressContract);
        let client = ProgressContractClient::new(&env, &id);

        let caller = Address::generate(&env);
        let result = client.try_advance_level(&caller, &99u64, &1u32);

        assert_eq!(result, Err(Ok(ProgressError::NotInitialized)));
    }

    #[test]
    fn test_get_progress_history_three_entries() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 10u64;

        client.advance_level(&validator, &player_id, &1u32);
        client.advance_level(&validator, &player_id, &2u32);
        client.advance_level(&validator, &player_id, &3u32);

        let history = client.get_progress_history(&player_id);

        assert_eq!(history.len(), 3);
        assert_eq!(history.get(0).unwrap().old_level, ProgressLevel::Unverified);
        assert_eq!(history.get(0).unwrap().new_level, ProgressLevel::VerifiedIdentity);
        assert_eq!(history.get(0).unwrap().milestone_ref, 1u32);

        assert_eq!(history.get(1).unwrap().old_level, ProgressLevel::VerifiedIdentity);
        assert_eq!(history.get(1).unwrap().new_level, ProgressLevel::PerformanceMilestones);
        assert_eq!(history.get(1).unwrap().milestone_ref, 2u32);

        assert_eq!(history.get(2).unwrap().old_level, ProgressLevel::PerformanceMilestones);
        assert_eq!(history.get(2).unwrap().new_level, ProgressLevel::EliteTier);
        assert_eq!(history.get(2).unwrap().milestone_ref, 3u32);
    }

    #[test]
    fn test_get_progress_history_empty() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let history = client.get_progress_history(&999u64);
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_progress_updated_event_data() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 5u64;

        client.advance_level(&validator, &player_id, &1u32);

        let contract_id = client.address.clone();
        assert_eq!(
            env.events().all(),
            soroban_sdk::vec![
                &env,
                (
                    contract_id,
                    soroban_sdk::vec![
                        &env,
                        Symbol::new(&env, "progress_updated").into_val(&env),
                        validator.into_val(&env),
                    ],
                    (
                        player_id,
                        ProgressLevel::Unverified,
                        ProgressLevel::VerifiedIdentity,
                    )
                        .into_val(&env),
                )
            ]
        );
    }

    #[test]
    #[should_panic]
    fn test_cannot_exceed_elite_tier() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 1u64;

        client.advance_level(&validator, &player_id, &1u32);
        client.advance_level(&validator, &player_id, &2u32);
        client.advance_level(&validator, &player_id, &3u32);
        client.advance_level(&validator, &player_id, &4u32);
    }

    #[test]
    fn test_transfer_admin_success() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let new_admin = Address::generate(&env);
        client.transfer_admin(&new_admin);
    }

    #[test]
    #[should_panic]
    fn test_transfer_admin_unauthorized() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        env.mock_auths(&[]);
        client.transfer_admin(&Address::generate(&env));
    }

    #[test]
    fn test_pause_and_unpause() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 42u64;

        client.pause_contract();

        let err = client
            .try_advance_level(&validator, &player_id, &1u32)
            .expect_err("expected an error while paused");
        assert_eq!(err.unwrap(), ProgressError::ContractPaused, "expected ContractPaused error");

        assert_eq!(client.get_level(&player_id), ProgressLevel::Unverified);

        client.unpause_contract();

        let new_level = client.advance_level(&validator, &player_id, &1u32);
        assert_eq!(new_level, ProgressLevel::VerifiedIdentity);
    }

    #[test]
    #[should_panic]
    fn test_old_admin_loses_access_after_transfer() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let new_admin = Address::generate(&env);
        client.transfer_admin(&new_admin);

        env.mock_auths(&[]);
        client.pause_contract();
    }

    #[test]
    fn test_reset_player_level_success() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 1u64;

        client.advance_level(&validator, &player_id, &1u32);
        client.advance_level(&validator, &player_id, &2u32);
        assert_eq!(client.get_history_count(&player_id), 2);

        client.reset_player_level(&player_id, &ProgressLevel::Unverified);

        assert_eq!(
            env.events().all(),
            vec![
                &env,
                (
                    client.address.clone(),
                    (Symbol::new(&env, "player_level_reset"),).into_val(&env),
                    (
                        player_id,
                        ProgressLevel::PerformanceMilestones,
                        ProgressLevel::Unverified,
                    )
                        .into_val(&env),
                ),
            ]
        );

        assert_eq!(client.get_level(&player_id), ProgressLevel::Unverified);
        assert_eq!(client.get_history_count(&player_id), 3);

        let reset_entry = client.get_history_entry(&player_id, &3u32);
        assert_eq!(reset_entry.old_level, ProgressLevel::PerformanceMilestones);
        assert_eq!(reset_entry.new_level, ProgressLevel::Unverified);
        assert_eq!(reset_entry.updated_by, admin);
        assert_eq!(reset_entry.milestone_ref, 0);
    }

    #[test]
    #[should_panic]
    fn test_reset_player_level_unauthorized() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        env.mock_auths(&[]);
        client.reset_player_level(&1u64, &ProgressLevel::Unverified);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #8)")]
    fn test_advance_level_history_counter_overflow() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let caller = Address::generate(&env);
        let player_id = 1u64;

        env.as_contract(&client.address, || {
            env.storage()
                .persistent()
                .set(&DataKey::HistoryCounter(player_id), &u32::MAX);
        });

        client.advance_level(&caller, &player_id, &1u32);
    }

    // ---------------------------------------------------------------------------
    // Configurable history depth tests (Issue #8)
    // ---------------------------------------------------------------------------

    #[test]
    fn test_default_history_depth_is_10() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        assert_eq!(
            client.get_history_max_depth(),
            DEFAULT_HISTORY_MAX_DEPTH,
            "default depth must equal DEFAULT_HISTORY_MAX_DEPTH (10)"
        );
    }

    #[test]
    fn test_set_history_max_depth_increases_ring() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // Set depth to 20 then write 15 entries.
        client.set_history_max_depth(&20u32);
        assert_eq!(client.get_history_max_depth(), 20u32);

        let validator = Address::generate(&env);
        let player_id = 50u64;

        // Advance through all 3 tiers then reset repeatedly to generate entries.
        // Use reset_player_level to keep writing history beyond the 3 natural tiers.
        client.advance_level(&validator, &player_id, &1u32); // 1
        client.advance_level(&validator, &player_id, &2u32); // 2
        client.advance_level(&validator, &player_id, &3u32); // 3

        for _ in 0..12 {
            client.reset_player_level(&player_id, &ProgressLevel::Unverified);
        }
        // Total entries written: 3 advances + 12 resets = 15
        assert_eq!(client.get_history_count(&player_id), 15);

        // With depth=20 none should have been evicted: all 15 must be readable.
        for i in 1u32..=15u32 {
            // Should not panic.
            let _ = client.get_history_entry(&player_id, &i);
        }
    }

    #[test]
    fn test_set_history_max_depth_decreases_ring_on_next_write() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 60u64;

        // Write 5 entries at default depth (10): indices 1..=5.
        client.advance_level(&validator, &player_id, &1u32); // 1
        client.advance_level(&validator, &player_id, &2u32); // 2
        client.advance_level(&validator, &player_id, &3u32); // 3
        client.reset_player_level(&player_id, &ProgressLevel::Unverified); // 4
        client.reset_player_level(&player_id, &ProgressLevel::Unverified); // 5

        // Reduce depth to 3.
        client.set_history_max_depth(&3u32);

        // One more write (entry 6) should evict entry 3 (6 - 3 = 3).
        client.reset_player_level(&player_id, &ProgressLevel::Unverified); // 6

        // Entry 3 must now be gone.
        let evicted = client.try_get_history_entry(&player_id, &3u32);
        assert!(evicted.is_err(), "entry 3 should have been evicted");

        // The three most-recent entries (4, 5, 6) must still be present.
        let _ = client.get_history_entry(&player_id, &4u32);
        let _ = client.get_history_entry(&player_id, &5u32);
        let _ = client.get_history_entry(&player_id, &6u32);

        // history_count still reflects the monotonic counter (6).
        assert_eq!(client.get_history_count(&player_id), 6);
    }

    #[test]
    fn test_history_depth_zero_rejected() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let result = client.try_set_history_max_depth(&0u32);
        assert_eq!(result, Err(Ok(ProgressError::InvalidHistoryDepth)));
    }

    #[test]
    fn test_history_depth_above_ceiling_rejected() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let result = client.try_set_history_max_depth(&51u32);
        assert_eq!(result, Err(Ok(ProgressError::InvalidHistoryDepth)));
    }

    #[test]
    fn test_history_depth_at_ceiling_accepted() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // MAX_HISTORY_DEPTH = 50 must be accepted.
        let result = client.try_set_history_max_depth(&MAX_HISTORY_DEPTH);
        assert!(result.is_ok(), "depth equal to MAX_HISTORY_DEPTH must be accepted");
        assert_eq!(client.get_history_max_depth(), MAX_HISTORY_DEPTH);
    }

    #[test]
    #[should_panic]
    fn test_set_history_max_depth_requires_admin() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        env.mock_auths(&[]);
        client.set_history_max_depth(&20u32);
    }
}
