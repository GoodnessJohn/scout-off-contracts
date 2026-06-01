use soroban_sdk::{Address, Env, Symbol, String};

pub fn milestone_approved(
    env: &Env,
    player_id: u64,
    validator: &Address,
    milestone_index: u32,
    description: &String,
    evidence_hash: &String,
) {
    env.events().publish(
        (
            Symbol::new(env, "milestone_approved"),
            validator.clone(),
            milestone_index,
        ),
        (player_id, description.clone(), evidence_hash.clone()),
    );
}

pub fn validator_registered(env: &Env, wallet: &Address) {
    env.events().publish(
        (Symbol::new(env, "validator_registered"),),
        wallet.clone(),
    );
}

pub fn validator_revoked(env: &Env, wallet: &Address) {
    env.events().publish(
        (Symbol::new(env, "validator_revoked"),),
        wallet.clone(),
    );
}

pub fn contract_paused(env: &Env, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "contract_paused"),),
        admin.clone(),
    );
}

pub fn contract_unpaused(env: &Env, admin: &Address) {
    env.events().publish(
        (Symbol::new(env, "contract_unpaused"),),
        admin.clone(),
    );
}