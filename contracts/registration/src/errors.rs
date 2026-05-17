use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
#[repr(u32)]
pub enum ScoutChainError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    PlayerNotFound = 3,
    ValidatorNotAuthorized = 4,
    InvalidProgressTransition = 5,
    ScoutNotSubscribed = 6,
    InsufficientFee = 7,
    AlreadyRegistered = 8,
    ContractPaused = 9,
    Unauthorized = 10,
    Overflow = 11,
    ScoutNotFound = 12,
    InvalidInput = 13,
}
