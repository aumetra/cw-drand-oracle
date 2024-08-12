use cosmwasm_std::{StdError, VerificationError};
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ContractError {
    #[error("Invalid signature")]
    InvalidSignature,

    #[error("Invalid randomness")]
    InvalidRandomness,

    #[error(transparent)]
    Std(#[from] StdError),

    #[error(transparent)]
    Verification(#[from] VerificationError),
}
