#![cfg(test)]

use crate::*;
use soroban_sdk::{testutils::Address as _, Address, Bytes, Env};

fn setup() -> (Env, EscrowContractClient<'static>, Address) {
    let env = Env::default();
    let id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(&env, &id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.initialize(&admin);
    (env, client, admin)
}

#[test]
fn test_direct_update_required_signatures_rejected() {
    let (env, client, admin) = setup();
    let _ = env;
    let result = client.try_update_required_signatures(&admin, &1u32);
    assert_eq!(result, Err(Ok(Error::Unauthorized)));
}

#[test]
fn test_threshold_change_via_multisig() {
    let (env, client, admin) = setup();

    let admin2 = Address::generate(&env);
    client.add_admin(&admin, &admin2);

    // With threshold=1 (default), one approval from proposer is enough to execute.
    let proposal_id = client.propose_action(
        &admin,
        &ActionType::UpdateThreshold { new_threshold: 2u32 },
        &admin2,
        &Bytes::new(&env),
    );
    client.execute_action(&proposal_id);

    let config = client.get_multisig_config();
    assert_eq!(config.required_signatures, 2);
}

#[test]
fn test_threshold_zero_rejected() {
    let (env, client, admin) = setup();

    let proposal_id = client.propose_action(
        &admin,
        &ActionType::UpdateThreshold { new_threshold: 0u32 },
        &admin,
        &Bytes::new(&env),
    );

    let result = client.try_execute_action(&proposal_id);
    assert_eq!(result, Err(Ok(Error::InvalidThreshold)));
}

#[test]
fn test_threshold_exceeds_admin_count_rejected() {
    let (env, client, admin) = setup();

    // Only 1 admin exists; requesting threshold=2 should fail.
    let proposal_id = client.propose_action(
        &admin,
        &ActionType::UpdateThreshold { new_threshold: 2u32 },
        &admin,
        &Bytes::new(&env),
    );

    let result = client.try_execute_action(&proposal_id);
    assert_eq!(result, Err(Ok(Error::InsufficientAdmins)));
}
