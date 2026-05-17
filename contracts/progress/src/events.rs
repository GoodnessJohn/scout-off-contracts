use soroban_sdk::{Address, Env, Symbol};
use crate::types::ProgressLevel;

pub fn progress_updated(
    env: &Env,
    player_id: u64,
    new_level: &ProgressLevel,
    updated_by: &Address,
) {
    env.events().publish(
        (
            Symbol::new(env, "progress_updated"),
            updated_by.clone(),
        ),
        (player_id, new_level.clone()),
    );
}
