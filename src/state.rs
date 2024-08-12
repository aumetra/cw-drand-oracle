use cosmwasm_schema::cw_serde;
use cw_storage_plus::Map;

pub const BEACONS: Map<u64, Randomness> = Map::new("beacons");

#[cw_serde]
pub struct Randomness {
    pub uniform_seed: [u8; 32],
}
