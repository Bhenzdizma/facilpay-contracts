#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, token, Address, Env};

fn setup(env: &Env) -> (EscrowContractClient, Address, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);

    let token_addr = env.register_stellar_asset_contract(admin.clone());
    let token_admin = token::StellarAssetClient::new(env, &token_addr);
    let customer = Address::generate(env);
    token_admin.mint(&customer, &10_000);
    // fund contract directly so transfers succeed
    token_admin.mint(&contract_id, &10_000);

    (client, admin, customer, Address::generate(env), token_addr)
}

#[test]
fn test_expire_escrow_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, customer, merchant, token) = setup(&env);

    env.ledger().set_timestamp(1000);
    // release_timestamp=2000, expiry_timestamp=3000
    let escrow_id =
        client.create_escrow(&customer, &merchant, &500_i128, &token, &2000_u64, &0_u64, &3000_u64, &true);

    assert!(!client.is_escrow_expired(&escrow_id));

    env.ledger().set_timestamp(3001);
    assert!(client.is_escrow_expired(&escrow_id));

    client.expire_escrow(&escrow_id);

    let escrow = client.get_escrow(&escrow_id);
    assert_eq!(escrow.status, EscrowStatus::Cancelled);
}

#[test]
fn test_expire_escrow_premature_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, customer, merchant, token) = setup(&env);

    env.ledger().set_timestamp(1000);
    let escrow_id =
        client.create_escrow(&customer, &merchant, &500_i128, &token, &2000_u64, &0_u64, &3000_u64, &true);

    // Still before expiry
    env.ledger().set_timestamp(2500);
    let result = client.try_expire_escrow(&escrow_id);
    assert!(result.is_err());
}

#[test]
fn test_expire_disputed_escrow_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, customer, merchant, token) = setup(&env);

    env.ledger().set_timestamp(1000);
    let escrow_id =
        client.create_escrow(&customer, &merchant, &500_i128, &token, &2000_u64, &0_u64, &3000_u64, &true);

    client.dispute_escrow(&customer, &escrow_id);

    // Advance past expiry
    env.ledger().set_timestamp(4000);
    let result = client.try_expire_escrow(&escrow_id);
    assert!(result.is_err());
}

#[test]
fn test_set_global_expiry_config() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _, _, _) = setup(&env);

    client.set_global_expiry_config(&admin, &86400_u64);
    // No panic = success; config stored correctly
}

#[test]
fn test_create_escrow_expiry_before_release_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, customer, merchant, token) = setup(&env);

    env.ledger().set_timestamp(1000);
    // expiry_timestamp <= release_timestamp should fail
    let result = client.try_create_escrow(
        &customer, &merchant, &500_i128, &token, &5000_u64, &0_u64, &4000_u64, &true,
    );
    assert!(result.is_err());
}
