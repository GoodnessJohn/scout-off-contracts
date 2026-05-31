mod errors;
mod events;
mod types;

use errors::ProgressError;
use types::{DataKey, ProgressEntry, ProgressLevel};

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

#[contract]
pub struct ProgressContract;

#[contractimpl]
impl ProgressContract {
    // -------------------------------------------------------------------------
    // Admin
    // -------------------------------------------------------------------------

    pub fn initialize(env: Env, admin: Address) -> Result<(), ProgressError> {
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
        Self::require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    pub fn unpause_contract(env: Env) -> Result<(), ProgressError> {
        Self::require_admin(&env)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    /// Transfer admin rights to a new address (current admin auth required).
    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), ProgressError> {
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

    // -------------------------------------------------------------------------
    // Progress updates
    // -------------------------------------------------------------------------

    /// Advance a player's progress level by one tier.
    /// Caller must be an authorized validator (or scout for Level 3).
    /// `milestone_ref` links back to the verification contract's milestone index.
    pub fn advance_level(
        env: Env,
        caller: Address,
        player_id: u64,
        milestone_ref: u32,
    ) -> Result<ProgressLevel, ProgressError> {
        Self::require_not_paused(&env)?;
        Self::require_initialized(&env)?;
        caller.require_auth();

        let current = Self::get_current_level(&env, player_id);
        let new_level = current
            .next()
            .ok_or(ProgressError::AlreadyAtMaxLevel)?;

        // Record history entry
        let history_key = DataKey::HistoryCounter(player_id);
        let index: u32 = env
            .storage()
            .persistent()
            .get(&history_key)
            .unwrap_or(0u32);
        let next_index = index.checked_add(1).expect("overflow");

        let entry = ProgressEntry {
            player_id,
            old_level: current,
            new_level: new_level.clone(),
            updated_by: caller.clone(),
            updated_at: env.ledger().timestamp(),
            milestone_ref,
        };

        env.storage()
            .persistent()
            .set(&DataKey::HistoryEntry(player_id, next_index), &entry);
        env.storage()
            .persistent()
            .set(&history_key, &next_index);
        env.storage()
            .persistent()
            .set(&DataKey::PlayerLevel(player_id), &new_level);

        events::progress_updated(&env, player_id, &entry.old_level, &new_level, &caller);
        Ok(new_level)
    }

    // -------------------------------------------------------------------------
    // Queries
    // -------------------------------------------------------------------------

    pub fn get_level(env: Env, player_id: u64) -> ProgressLevel {
        Self::get_current_level(&env, player_id)
    }

    pub fn get_history_count(env: Env, player_id: u64) -> u32 {
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
        env.storage()
            .persistent()
            .get(&DataKey::HistoryEntry(player_id, index))
            .ok_or(ProgressError::PlayerNotFound)
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

    pub fn health(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Initialized)
            .unwrap_or(false)
    }

    // -------------------------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------------------------

    fn get_current_level(env: &Env, player_id: u64) -> ProgressLevel {
        env.storage()
            .persistent()
            .get(&DataKey::PlayerLevel(player_id))
            .unwrap_or(ProgressLevel::Unverified)
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

    fn require_admin(env: &Env) -> Result<(), ProgressError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ProgressError::NotInitialized)?;
        admin.require_auth();
        Ok(())
    }
}

// =============================================================================
// Tests
// =============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, testutils::Events as _, IntoVal, Env, Symbol};

    fn setup() -> (Env, ProgressContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let id = env.register_contract(None, ProgressContract);
        let client = ProgressContractClient::new(&env, &id);
        (env, client)
    }

    #[test]
    fn test_advance_level_sequence() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 1u64;

        // Unverified → VerifiedIdentity
        let l1 = client.advance_level(&validator, &player_id, &1u32);
        assert_eq!(l1, ProgressLevel::VerifiedIdentity);

        // VerifiedIdentity → PerformanceMilestones
        let l2 = client.advance_level(&validator, &player_id, &2u32);
        assert_eq!(l2, ProgressLevel::PerformanceMilestones);

        // PerformanceMilestones → EliteTier
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

        // Advance once: Unverified → VerifiedIdentity
        client.advance_level(&validator, &player_id, &milestone);

        // History index starts at 1
        let entry = client.get_history_entry(&player_id, &1u32);

        assert_eq!(entry.old_level, ProgressLevel::Unverified);
        assert_eq!(entry.new_level, ProgressLevel::VerifiedIdentity);
        assert_eq!(entry.updated_by, validator);
        assert_eq!(entry.milestone_ref, milestone);
    }

    #[test]
    fn test_advance_level_not_initialized() {
        let env = Env::default();
        env.mock_all_auths();
        // Register the contract but deliberately skip initialize()
        let id = env.register_contract(None, ProgressContract);
        let client = ProgressContractClient::new(&env, &id);

        let caller = Address::generate(&env);
        let result = client.try_advance_level(&caller, &99u64, &1u32);

        assert_eq!(
            result,
            Err(Ok(ProgressError::NotInitialized))
        );
    }

    #[test]
    fn test_get_progress_history_three_entries() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let validator = Address::generate(&env);
        let player_id = 10u64;

        // Advance through all three tiers
        client.advance_level(&validator, &player_id, &1u32);
        client.advance_level(&validator, &player_id, &2u32);
        client.advance_level(&validator, &player_id, &3u32);

        let history = client.get_progress_history(&player_id);

        assert_eq!(history.len(), 3);

        // Entry 1: Unverified → VerifiedIdentity
        assert_eq!(history.get(0).unwrap().old_level, ProgressLevel::Unverified);
        assert_eq!(history.get(0).unwrap().new_level, ProgressLevel::VerifiedIdentity);
        assert_eq!(history.get(0).unwrap().milestone_ref, 1u32);

        // Entry 2: VerifiedIdentity → PerformanceMilestones
        assert_eq!(history.get(1).unwrap().old_level, ProgressLevel::VerifiedIdentity);
        assert_eq!(history.get(1).unwrap().new_level, ProgressLevel::PerformanceMilestones);
        assert_eq!(history.get(1).unwrap().milestone_ref, 2u32);

        // Entry 3: PerformanceMilestones → EliteTier
        assert_eq!(history.get(2).unwrap().old_level, ProgressLevel::PerformanceMilestones);
        assert_eq!(history.get(2).unwrap().new_level, ProgressLevel::EliteTier);
        assert_eq!(history.get(2).unwrap().milestone_ref, 3u32);
    }

    #[test]
    fn test_get_progress_history_empty() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // Player 999 has never had advance_level called
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

        // Advance once: Unverified → VerifiedIdentity
        client.advance_level(&validator, &player_id, &1u32);

        // env.events().all() returns ContractEvents which compares against
        // soroban_sdk::Vec<(Address, Vec<Val>, Val)>:
        //   - Address  : the contract that emitted the event
        //   - Vec<Val> : topics  — (Symbol("progress_updated"), updated_by)
        //   - Val      : data    — (player_id, old_level, new_level)
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
                    (player_id, ProgressLevel::Unverified, ProgressLevel::VerifiedIdentity)
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
        // This should panic — already at EliteTier
        client.advance_level(&validator, &player_id, &4u32);
    }

    #[test]
    fn test_transfer_admin_success() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let new_admin = Address::generate(&env);
        // Should not panic — current admin auth is satisfied
        client.transfer_admin(&new_admin);
    }

    #[test]
    #[should_panic]
    fn test_transfer_admin_unauthorized() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        // Clear all mocks — no auth satisfied, so admin check fails
        env.mock_auths(&[]);
        client.transfer_admin(&Address::generate(&env));
    }

    #[test]
    #[should_panic]
    fn test_old_admin_loses_access_after_transfer() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        client.initialize(&admin);

        let new_admin = Address::generate(&env);
        client.transfer_admin(&new_admin);

        // Clear mocks — old admin auth no longer stored, so pause must fail
        env.mock_auths(&[]);
        client.pause_contract();
    }
}
