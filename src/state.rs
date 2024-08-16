use cosmwasm_schema::cw_serde;
use cosmwasm_std::Addr;
use cw_storage_plus::Map;
use std::collections::HashSet;

pub const BEACONS: Map<u64, Randomness> = Map::new("beacons");
pub const DELIVERY_QUEUES: Map<u64, DeliveryQueue> = Map::new("delivery_queues");

#[cw_serde]
pub struct Randomness {
    pub uniform_seed: [u8; 32],
}

#[cw_serde]
#[derive(Default)]
pub struct DeliveryQueue {
    pub receivers: HashSet<Addr>,
}
