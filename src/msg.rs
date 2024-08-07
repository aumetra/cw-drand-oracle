use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{HexBinary, Uint64};

#[cw_serde]
pub struct InstantiateMsg {}

#[cw_serde]
pub enum ExecuteMsg {
    AddBeacon {
        round: Uint64,
        signature: HexBinary,
        randomness: HexBinary,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    /// Get a particular beacon by its round
    #[returns(BeaconResponse)]
    Beacon { round: Uint64 },

    // Get the latest beacon known to the contract
    #[returns(LatestBeaconResponse)]
    LatestBeacon {},
}

#[cw_serde]
pub struct BeaconResponse {
    pub uniform_seed: [u8; 32],
}

#[cw_serde]
pub struct LatestBeaconResponse {
    pub round: Uint64,
    pub uniform_seed: [u8; 32],
}
