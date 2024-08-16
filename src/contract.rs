#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdError, StdResult,
};

use crate::error::ContractError;
use crate::msg::{ExecuteMsg, InstantiateMsg, QueryMsg};
use crate::state::BEACONS;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:drand-oracle";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    cw2::set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::AddBeacon {
            round,
            signature,
            randomness,
        } => execute::add_beacon(deps, round, signature, randomness),
        ExecuteMsg::NextRandomness {} => execute::next_beacon(deps, env, info),
    }
}

pub mod execute {
    use crate::{
        msg::ConcreteBeacon,
        state::{Randomness, DELIVERY_QUEUES},
    };
    use cosmwasm_std::{HashFunction, HexBinary, SubMsg, Timestamp, Uint64, WasmMsg};
    use hex_literal::hex;
    use sha2::{Digest, Sha256};

    use super::*;

    const G1_DOMAIN: &[u8] = b"BLS_SIG_BLS12381G1_XMD:SHA-256_SSWU_RO_NUL_";
    const QUICKNET_PUBLIC_KEY: [u8; 96] = hex!("83cf0f2896adee7eb8b5f01fcad3912212c437e0073e911fb90022d3e760183c8c4b450b6a0a6c3ac6a5776a2d1064510d1fec758c921cc22b0e17e63aaf4bcb5ed66304de9cf809bd274ca73bab4af5a6e9c76a4bc09e76eae8991ef5ece45a");

    const GENESIS: Timestamp = Timestamp::from_seconds(1692803367);
    const PERIOD_IN_NS: u64 = 3_000_000_000;

    const GAS_LIMIT: u64 = 10_000_000;

    fn next_round(now: Timestamp) -> u64 {
        if now < GENESIS {
            1
        } else {
            let from_genesis = now.nanos() - GENESIS.nanos();
            let periods_since_genesis = from_genesis / PERIOD_IN_NS;
            periods_since_genesis + 1 + 1 // Second addition to convert to 1-based counting
        }
    }

    pub fn add_beacon(
        deps: DepsMut,
        round: Uint64,
        signature: HexBinary,
        randomness: HexBinary,
    ) -> Result<Response, ContractError> {
        // Verify the randomness beacon
        let msg = Sha256::digest(round.to_be_bytes());
        let msg = deps
            .api
            .bls12_381_hash_to_g1(HashFunction::Sha256, &msg, G1_DOMAIN)?;

        let is_valid = deps.api.bls12_381_pairing_equality(
            &signature,
            &cosmwasm_std::BLS12_381_G2_GENERATOR,
            &msg,
            &QUICKNET_PUBLIC_KEY,
        )?;
        if !is_valid {
            return Err(ContractError::InvalidSignature);
        }

        let reproduced_randomness = Sha256::digest(&signature);
        if reproduced_randomness.as_slice() != randomness.as_slice() {
            return Err(ContractError::InvalidRandomness);
        }

        // Store the randomness beacon
        BEACONS.save(
            deps.storage,
            round.u64(),
            &Randomness {
                uniform_seed: reproduced_randomness.into(),
            },
        )?;

        let mut response: Response = Response::new();

        // Load from the job queue and send the beacon to all receivers
        if let Some(queue) = DELIVERY_QUEUES.may_load(deps.storage, round.u64())? {
            for receiver in queue.receivers {
                response = response.add_submessage(
                    SubMsg::new(WasmMsg::Execute {
                        contract_addr: receiver.into(),
                        msg: cosmwasm_std::to_json_binary(&ConcreteBeacon {
                            round,
                            uniform_seed: reproduced_randomness.into(),
                        })?,
                        funds: vec![],
                    })
                    .with_gas_limit(GAS_LIMIT),
                );
            }
        }

        // Delete the job queue
        DELIVERY_QUEUES.remove(deps.storage, round.u64());

        Ok(Response::default())
    }

    pub fn next_beacon(
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
    ) -> Result<Response, ContractError> {
        let next_round = next_round(env.block.time);
        let mut queue = DELIVERY_QUEUES
            .may_load(deps.storage, next_round)?
            .unwrap_or_default();
        queue.receivers.insert(info.sender);

        DELIVERY_QUEUES.save(deps.storage, next_round, &queue)?;

        Ok(Response::default())
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Beacon { round } => to_json_binary(&query::beacon(deps, round)?),
        QueryMsg::LatestBeacon {} => to_json_binary(&query::latest_beacon(deps)?),
    }
}

pub mod query {
    use crate::msg::{BeaconResponse, ConcreteBeacon};

    use super::*;
    use cosmwasm_std::Uint64;

    pub fn beacon(deps: Deps, round: Uint64) -> StdResult<BeaconResponse> {
        BEACONS
            .load(deps.storage, round.u64())
            .map(|beacon| BeaconResponse {
                uniform_seed: beacon.uniform_seed,
            })
    }

    pub fn latest_beacon(deps: Deps) -> StdResult<ConcreteBeacon> {
        BEACONS
            .last(deps.storage)
            .transpose()
            .ok_or_else(|| StdError::not_found("no known beacons"))?
            .map(|(round, beacon)| ConcreteBeacon {
                round: round.into(),
                uniform_seed: beacon.uniform_seed,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::msg::{BeaconResponse, ConcreteBeacon};
    use cosmwasm_std::{
        from_json,
        testing::{mock_dependencies, mock_env},
        Addr, HexBinary, Uint64,
    };
    use hex_literal::hex;

    const ROUND: u64 = 123;
    const SIGNATURE: [u8; 48] = hex!("b75c69d0b72a5d906e854e808ba7e2accb1542ac355ae486d591aa9d43765482e26cd02df835d3546d23c4b13e0dfc92");
    const RANDOMNESS: [u8; 32] =
        hex!("fb8f7bc29bf24db51871ec8c79f3a1e4bd0557bc0dfcee9ed1d924e69d1c60dc");

    fn add_beacon_msg() -> ExecuteMsg {
        ExecuteMsg::AddBeacon {
            round: Uint64::new(ROUND),
            signature: HexBinary::from(SIGNATURE),
            randomness: HexBinary::from(RANDOMNESS),
        }
    }

    fn message_info() -> MessageInfo {
        cosmwasm_std::testing::message_info(&Addr::unchecked("anyone"), &[])
    }

    #[test]
    fn accepts_beacon() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        execute(deps.as_mut(), env, message_info(), add_beacon_msg()).unwrap();
    }

    #[test]
    fn rejects_invalid_signature() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let mut msg = add_beacon_msg();
        let ExecuteMsg::AddBeacon { signature, .. } = &mut msg else {
            unreachable!();
        };

        let mut sig = Vec::from(signature.clone());
        sig[0] ^= 0xF3;

        *signature = HexBinary::from(sig);

        let res = execute(deps.as_mut(), env, message_info(), msg);
        assert!(res.is_err());
    }

    #[test]
    fn rejects_invalid_randomness() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        let mut msg = add_beacon_msg();
        let ExecuteMsg::AddBeacon { randomness, .. } = &mut msg else {
            unreachable!();
        };

        let mut rand = Vec::from(randomness.clone());
        rand[0] ^= 0xF3;

        *randomness = HexBinary::from(rand);

        let res = execute(deps.as_mut(), env, message_info(), msg);
        assert_eq!(res.unwrap_err(), ContractError::InvalidRandomness);
    }

    #[test]
    fn latest_beacon() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        execute(deps.as_mut(), env.clone(), message_info(), add_beacon_msg()).unwrap();

        let res = query(deps.as_ref(), env, QueryMsg::LatestBeacon {}).unwrap();
        let value: ConcreteBeacon = from_json(&res).unwrap();
        assert_eq!(value.uniform_seed, RANDOMNESS);
    }

    #[test]
    fn get_beacon() {
        let mut deps = mock_dependencies();
        let env = mock_env();

        execute(deps.as_mut(), env.clone(), message_info(), add_beacon_msg()).unwrap();

        let res = query(
            deps.as_ref(),
            env,
            QueryMsg::Beacon {
                round: Uint64::new(123),
            },
        )
        .unwrap();

        let value: BeaconResponse = from_json(&res).unwrap();
        assert_eq!(value.uniform_seed, RANDOMNESS);
    }
}
