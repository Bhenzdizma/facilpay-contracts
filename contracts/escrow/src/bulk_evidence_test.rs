#![cfg(test)]

use crate::*;
use soroban_sdk::{testutils::Address as _, token, Address, Bytes, Env, Vec};

fn setup(env: &Env) -> (EscrowContractClient, Address, Address, Address, Address) {
    env.mock_all_auths();
    let id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &id);
    let admin = Address::generate(env);
    client.initialize(&admin);

    let token_addr = env.register_stellar_asset_contract(admin.clone());
    let token_admin = token::StellarAssetClient::new(env, &token_addr);
    let customer = Address::generate(env);
    token_admin.mint(&customer, &10_000i128);
    token_admin.mint(&id, &10_000i128);

    (client, admin, customer, Address::generate(env), token_addr)
}

fn make_disputed_escrow(
    env: &Env,
    client: &EscrowContractClient,
    customer: &Address,
    merchant: &Address,
    token: &Address,
) -> u64 {
    let escrow_id = client.create_escrow(
        customer, merchant, &500i128, token,
        &9999u64, &0u64, &0u64, &false,
    );
    client.dispute_escrow(customer, &escrow_id);
    escrow_id
}

#[test]
fn test_batch_submission_stores_all_items() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut items: Vec<Bytes> = Vec::new(&env);
    items.push_back(Bytes::from_array(&env, &[1u8; 32]));
    items.push_back(Bytes::from_array(&env, &[2u8; 32]));
    items.push_back(Bytes::from_array(&env, &[3u8; 32]));

    let page_count = client.submit_evidence_batch(&customer, &escrow_id, &items);
    assert_eq!(page_count, 1u32);

    let page = client.get_evidence_page(&escrow_id, &0u32);
    assert_eq!(page.len(), 3u32);
}

#[test]
fn test_oversized_batch_rejected() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut items: Vec<Bytes> = Vec::new(&env);
    for _ in 0..11u32 {
        items.push_back(Bytes::from_array(&env, &[0u8; 32]));
    }

    let result = client.try_submit_evidence_batch(&customer, &escrow_id, &items);
    assert_eq!(result, Err(Ok(Error::BatchTooLarge)));
}

#[test]
fn test_pagination_two_pages() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    let mut batch1: Vec<Bytes> = Vec::new(&env);
    for _ in 0..10u32 {
        batch1.push_back(Bytes::from_array(&env, &[1u8; 32]));
    }
    client.submit_evidence_batch(&customer, &escrow_id, &batch1);

    let mut batch2: Vec<Bytes> = Vec::new(&env);
    batch2.push_back(Bytes::from_array(&env, &[2u8; 32]));
    let page_count = client.submit_evidence_batch(&merchant, &escrow_id, &batch2);
    assert_eq!(page_count, 2u32);

    assert_eq!(client.get_evidence_page(&escrow_id, &0u32).len(), 10u32);
    assert_eq!(client.get_evidence_page(&escrow_id, &1u32).len(), 1u32);
}

#[test]
fn test_backward_compat_get_evidence_still_works() {
    let env = Env::default();
    let (client, _admin, customer, merchant, token) = setup(&env);
    let escrow_id = make_disputed_escrow(&env, &client, &customer, &merchant, &token);

    client.submit_evidence(
        &customer,
        &escrow_id,
        &soroban_sdk::String::from_str(&env, "QmOldHash"),
    );

    let items = client.get_evidence(&escrow_id, &10u64, &0u64);
    assert_eq!(items.len(), 1u32);
    assert_eq!(
        items.get(0).unwrap().ipfs_hash,
        soroban_sdk::String::from_str(&env, "QmOldHash")
    );
}
