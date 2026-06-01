#![no_std]
mod errors;
mod events;
mod types;

use crate::errors::ScoutAccessError;
use crate::types::{DataKey, FeeConfig, Subscription, SubscriptionTier};
use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol};

#[contract]
pub struct ScoutAccessContract;

#[contractimpl]
impl ScoutAccessContract {
    pub fn initialize(env: Env, admin: Address, token: Address, config: FeeConfig) -> Result<(), ScoutAccessError> {
        if env.storage().instance().has(&DataKey::Initialized) {
            return Err(ScoutAccessError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::XlmToken, &token);
        env.storage().instance().set(&DataKey::FeeConfig, &config);
        env.storage().instance().set(&DataKey::Initialized, &true);
        Ok(())
    }

    pub fn subscribe(env: Env, scout: Address, tier: SubscriptionTier) -> Result<(), ScoutAccessError> {
        let admin = Self::get_admin(&env);
        admin.require_auth();

        let config = Self::get_fee_config(&env);
        let amount = match tier {
            SubscriptionTier::Basic => config.basic_sub_stroops,
            SubscriptionTier::Pro => config.pro_sub_stroops,
            SubscriptionTier::Elite => config.elite_sub_stroops,
        };

        let token = Self::get_token(&env);
        token::Client::new(&env, &token).transfer(&scout, &env.current_contract_address(), &amount);

        let sub = Subscription {
            scout: scout.clone(),
            tier,
            expires_at: env.ledger().timestamp() + config.sub_duration_secs,
            subscribed_at: env.ledger().timestamp(),
        };

        env.storage().persistent().set(&DataKey::Subscription(scout.clone()), &sub);
        
        let current_fees: i128 = env.storage().instance().get(&DataKey::AccumulatedFees).unwrap_or(0);
        env.storage().instance().set(&DataKey::AccumulatedFees, &(current_fees + amount));

        events::scout_subscribed(&env, &scout, &sub.tier);
        Ok(())
    }

    pub fn withdraw_fees(env: Env, to: Address) -> Result<(), ScoutAccessError> {
        Self::require_admin(&env)?;
        
        let amount: i128 = env.storage().instance().get(&DataKey::AccumulatedFees).unwrap_or(0);
        if amount == 0 { return Err(ScoutAccessError::NoFeesToWithdraw); }

        let token = Self::get_token(&env);
        token::Client::new(&env, &token).transfer(&env.current_contract_address(), &to, &amount);
        
        env.storage().instance().set(&DataKey::AccumulatedFees, &0i128);
        events::fees_withdrawn(&env, &to, amount);
        Ok(())
    }

    pub fn transfer_admin(env: Env, new_admin: Address) -> Result<(), ScoutAccessError> {
        Self::require_admin(&env)?;
        let old_admin = Self::get_admin(&env);
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        events::admin_transferred(&env, &old_admin, &new_admin);
        Ok(())
    }

    pub fn pay_to_contact(env: Env, scout: Address, player_id: u64) -> Result<(), ScoutAccessError> {
        scout.require_auth();
        let config = Self::get_fee_config(&env);
        let token = Self::get_token(&env);
        token::Client::new(&env, &token).transfer(&scout, &env.current_contract_address(), &config.contact_fee_stroops);

        env.storage().persistent().set(&DataKey::ContactRecord(player_id, scout.clone()), &true);
        let current_fees: i128 = env.storage().instance().get(&DataKey::AccumulatedFees).unwrap_or(0);
        env.storage().instance().set(&DataKey::AccumulatedFees, &(current_fees + config.contact_fee_stroops));
        
        events::player_contacted(&env, player_id, &scout);
        Ok(())
    }

    pub fn get_subscription(env: Env, scout: Address) -> Subscription {
        env.storage().persistent().get(&DataKey::Subscription(scout)).unwrap()
    }

    pub fn get_accumulated_fees(env: Env) -> i128 {
        env.storage().instance().get(&DataKey::AccumulatedFees).unwrap_or(0)
    }

    pub fn has_contacted(env: Env, scout: Address, player_id: u64) -> bool {
        env.storage().persistent().get(&DataKey::ContactRecord(player_id, scout)).unwrap_or(false)
    }

    pub fn health(env: Env) -> bool {
        env.storage().instance().get(&DataKey::Initialized).unwrap_or(false)
    }

    fn require_admin(env: &Env) -> Result<(), ScoutAccessError> {
        let admin = Self::get_admin(env);
        admin.require_auth();
        Ok(())
    }

    fn get_admin(env: &Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).ok_or(ScoutAccessError::NotInitialized).unwrap()
    }

    fn get_token(env: &Env) -> Address {
        env.storage().instance().get(&DataKey::XlmToken).unwrap()
    }

    fn get_fee_config(env: &Env) -> FeeConfig {
        env.storage().instance().get(&DataKey::FeeConfig).unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Events, Ledger},
        token::{Client as TokenClient, StellarAssetClient},
        Env, IntoVal, Symbol,
    };

    fn create_token(env: &Env, admin: &Address) -> Address {
        let token_id = env.register_stellar_asset_contract_v2(admin.clone());
        token_id.address()
    }

    fn mint_token(env: &Env, token: &Address, _admin: &Address, to: &Address, amount: i128) {
        StellarAssetClient::new(env, token).mint(to, &amount);
    }

    fn default_fees() -> FeeConfig {
        FeeConfig {
            contact_fee_stroops: 100_000,
            basic_sub_stroops: 1_000_000,
            pro_sub_stroops: 3_000_000,
            elite_sub_stroops: 7_000_000,
            sub_duration_secs: 30 * 24 * 60 * 60,
        }
    }

    fn setup() -> (Env, Address, Address, ScoutAccessContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let xlm = create_token(&env, &admin);
        let contract_id = env.register_contract(None, ScoutAccessContract);
        let client = ScoutAccessContractClient::new(&env, &contract_id);
        client.initialize(&admin, &xlm, &default_fees());
        (env, admin, xlm, client)
    }

    #[test]
    fn test_transfer_admin_success() {
        let (env, admin, _xlm, client) = setup();
        let new_admin = Address::generate(&env);
        
        client.transfer_admin(&new_admin);
        
        // Assert event
        let event = env.events().all().vec().last().unwrap();
        assert_eq!(event.0, client.address); // Contract ID
        assert_eq!(event.1.get(0).unwrap(), Symbol::new(&env, "admin_transferred").to_val());
        
        // Ensure new admin can perform admin action
        client.transfer_admin(&admin);
    }

    #[test]
    #[should_panic]
    fn test_transfer_admin_unauthorized() {
        let (env, _admin, _xlm, client) = setup();
        let new_admin = Address::generate(&env);
        let unauthorized = Address::generate(&env);
        
        env.mock_auths(&[(
            unauthorized.clone(),
            client.address.clone(),
            Symbol::new(&env, "transfer_admin"),
            (new_admin.clone(),).into_val(&env),
        )]);
        
        client.transfer_admin(&new_admin);
    }

    #[test]
    fn test_withdraw_fees_event() {
        let (env, admin, xlm, client) = setup();
        let scout = Address::generate(&env);
        mint_token(&env, &xlm, &admin, &scout, 10_000_000);
        client.subscribe(&scout, &SubscriptionTier::Basic);

        // Advance ledger
        env.ledger().with_mut(|l| l.timestamp = 12345);

        client.withdraw_fees(&admin);

        let event = env.events().all().vec().last().unwrap();
        let (_, topics, data) = (event.0, event.1, event.2);
        assert_eq!(topics.get(0).unwrap(), Symbol::new(&env, "fees_withdrawn").to_val());
        
        let (to, amount, timestamp): (Address, i128, u64) = data.unpack(&env);
        assert_eq!(to, admin);
        assert_eq!(amount, 1_000_000);
        assert_eq!(timestamp, 12345);
    }
}
