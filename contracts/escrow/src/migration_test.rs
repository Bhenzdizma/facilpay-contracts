#![cfg(test)]

use crate::*;
use soroban_sdk::testutils::Ledger;
use soroban_sdk::{testutils::Address as _, vec, Address, Env};

fn setup(env: &Env) -> (EscrowContractClient, Address, Address, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(EscrowContract, ());
    let client = EscrowContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(&admin);
    let customer = Address::generate(env);
    let merchant = Address::generate(env);
    let token = Address::generate(env);
    (client, admin, customer, merchant, token)
}

// ── MIGRATION FLOW ────────────────────────────────────────────────────────────

#[test]
fn test_begin_migration_sets_status() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    // Create a couple of escrows first
    client.create_escrow(&customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false);
    client.create_escrow(&customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false);

    env.ledger().set_timestamp(100);
    client.begin_migration(&admin);

    let status = client.get_migration_status();
    assert!(status.in_progress);
    assert_eq!(status.total_count, 2);
    assert_eq!(status.migrated_count, 0);
    assert_eq!(status.started_at, 100);
    assert!(status.completed_at.is_none());
}

#[test]
fn test_migrate_escrow_single() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let id = client.create_escrow(&customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false);
    client.begin_migration(&admin);
    client.migrate_escrow(&admin, &id);

    let status = client.get_migration_status();
    assert_eq!(status.migrated_count, 1);
}

#[test]
fn test_complete_migration_success() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let id1 = client.create_escrow(&customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false);
    let id2 = client.create_escrow(&customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false);

    env.ledger().set_timestamp(200);
    client.begin_migration(&admin);
    client.migrate_escrow(&admin, &id1);
    client.migrate_escrow(&admin, &id2);

    env.ledger().set_timestamp(300);
    client.complete_migration(&admin);

    let status = client.get_migration_status();
    assert!(!status.in_progress);
    assert_eq!(status.completed_at, Some(300));
}

#[test]
fn test_migrate_escrow_batch() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let id1 = client.create_escrow(&customer, &merchant, &100_i128, &token, &1000_u64, &0_u64, &0_u64, &false);
    let id2 = client.create_escrow(&customer, &merchant, &200_i128, &token, &1000_u64, &0_u64, &0_u64, &false);
    let id3 = client.create_escrow(&customer, &merchant, &300_i128, &token, &1000_u64, &0_u64, &0_u64, &false);

    client.begin_migration(&admin);

    let ids = vec![&env, id1, id2, id3];
    let count = client.migrate_escrow_batch(&admin, &ids);
    assert_eq!(count, 3);

    let status = client.get_migration_status();
    assert_eq!(status.migrated_count, 3);
}

// ── BLOCKED CREATION DURING MIGRATION ────────────────────────────────────────

#[test]
fn test_create_escrow_blocked_during_migration() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    client.begin_migration(&admin);

    let result = client.try_create_escrow(
        &customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false,
    );
    assert_eq!(result, Err(Ok(Error::ContractPaused)));
}

#[test]
fn test_create_escrow_allowed_after_migration_complete() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    // No escrows yet — begin and immediately complete
    client.begin_migration(&admin);
    client.complete_migration(&admin);

    // Should succeed now
    let id = client.create_escrow(&customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false);
    assert_eq!(id, 1);
}

// ── DOUBLE-MIGRATION GUARD ────────────────────────────────────────────────────

#[test]
fn test_double_migrate_returns_already_migrated() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let id = client.create_escrow(&customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false);
    client.begin_migration(&admin);
    client.migrate_escrow(&admin, &id);

    let result = client.try_migrate_escrow(&admin, &id);
    assert_eq!(result, Err(Ok(Error::AlreadyMigrated)));
}

#[test]
fn test_batch_skips_already_migrated() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let id1 = client.create_escrow(&customer, &merchant, &100_i128, &token, &1000_u64, &0_u64, &0_u64, &false);
    let id2 = client.create_escrow(&customer, &merchant, &200_i128, &token, &1000_u64, &0_u64, &0_u64, &false);

    client.begin_migration(&admin);
    client.migrate_escrow(&admin, &id1); // migrate id1 first

    // Batch includes id1 again — should skip it, only count id2
    let ids = vec![&env, id1, id2];
    let count = client.migrate_escrow_batch(&admin, &ids);
    assert_eq!(count, 1);

    let status = client.get_migration_status();
    assert_eq!(status.migrated_count, 2); // 1 from single + 1 from batch
}

// ── COMPLETE FAILS IF NOT ALL MIGRATED ───────────────────────────────────────

#[test]
fn test_complete_migration_fails_if_not_all_migrated() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    client.create_escrow(&customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false);
    client.create_escrow(&customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false);

    client.begin_migration(&admin);
    // Only migrate one of two
    client.migrate_escrow(&admin, &1);

    let result = client.try_complete_migration(&admin);
    assert!(result.is_err());
}

// ── MIGRATE WITHOUT BEGIN ─────────────────────────────────────────────────────

#[test]
fn test_migrate_without_begin_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, customer, merchant, token) = setup(&env);

    let id = client.create_escrow(&customer, &merchant, &500_i128, &token, &1000_u64, &0_u64, &0_u64, &false);

    let result = client.try_migrate_escrow(&admin, &id);
    assert_eq!(result, Err(Ok(Error::MigrationNotStarted)));
}

// ── UNAUTHORIZED ADMIN ────────────────────────────────────────────────────────

#[test]
fn test_begin_migration_unauthorized() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _customer, _merchant, _token) = setup(&env);

    let not_admin = Address::generate(&env);
    let result = client.try_begin_migration(&not_admin);
    assert_eq!(result, Err(Ok(Error::NotAnAdmin)));
}
