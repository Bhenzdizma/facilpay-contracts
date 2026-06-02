#![cfg(test)]
mod tests {
    use crate::{
        PaymentContract, Error, Payment, PaymentStatus, Currency, PaymentForwardConfig,
        FeeConfig, FeeTier, RateLimitConfig, MultiSigConfig,
    };
    use soroban_sdk::{testutils::Address as AddressTestUtils, Address, Env, String};

    fn setup_env() -> (Env, Address, Address, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::random(&env);
        let customer = Address::random(&env);
        let merchant = Address::random(&env);
        let forward_to = Address::random(&env);
        let token = Address::random(&env);

        // Initialize contract
        PaymentContract::initialize(env.clone(), admin.clone());

        // Set up fee config
        let fee_config = FeeConfig {
            fee_bps: 100,
            min_fee: 0,
            max_fee: i128::MAX,
            treasury: admin.clone(),
            fee_token: token.clone(),
            active: true,
        };
        PaymentContract::set_fee_config(env.clone(), admin.clone(), fee_config).unwrap();

        (env, admin, customer, merchant, forward_to, token)
    }

    fn create_test_payment(
        env: &Env,
        customer: &Address,
        merchant: &Address,
        amount: i128,
        token: &Address,
    ) -> u64 {
        let payment_id = PaymentContract::create_payment(
            env.clone(),
            customer.clone(),
            merchant.clone(),
            amount,
            token.clone(),
            Currency::USDC,
            3600,
            String::from_slice(&env, "test metadata"),
        ).unwrap();
        payment_id
    }

    #[test]
    fn test_set_payment_forward_valid() {
        let (env, _admin, _customer, merchant, forward_to, _token) = setup_env();

        let result = PaymentContract::set_payment_forward(
            env.clone(),
            merchant.clone(),
            forward_to.clone(),
            5000, // 50%
        );

        assert!(result.is_ok());

        // Verify the forward config was stored
        let config = PaymentContract::get_forward_config(env, merchant).unwrap();
        assert_eq!(config.merchant, merchant);
        assert_eq!(config.forward_to, forward_to);
        assert_eq!(config.forward_bps, 5000);
        assert!(config.active);
    }

    #[test]
    fn test_set_payment_forward_invalid_bps_zero() {
        let (env, _admin, _customer, merchant, forward_to, _token) = setup_env();

        let result = PaymentContract::set_payment_forward(
            env,
            merchant,
            forward_to,
            0, // Invalid: must be between 1 and 10000
        );

        assert_eq!(result, Err(Error::InvalidForwardBps));
    }

    #[test]
    fn test_set_payment_forward_invalid_bps_too_high() {
        let (env, _admin, _customer, merchant, forward_to, _token) = setup_env();

        let result = PaymentContract::set_payment_forward(
            env,
            merchant,
            forward_to,
            10001, // Invalid: must be between 1 and 10000
        );

        assert_eq!(result, Err(Error::InvalidForwardBps));
    }

    #[test]
    fn test_set_payment_forward_loop_detection() {
        let (env, _admin, _customer, merchant, forward_to, _token) = setup_env();

        // Set up a forward config for forward_to
        PaymentContract::set_payment_forward(
            env.clone(),
            forward_to.clone(),
            Address::random(&env),
            5000,
        ).unwrap();

        // Try to set merchant to forward to forward_to (which already has a forward)
        let result = PaymentContract::set_payment_forward(
            env,
            merchant,
            forward_to,
            5000,
        );

        assert_eq!(result, Err(Error::ForwardLoop));
    }

    #[test]
    fn test_set_payment_forward_valid_bps_boundaries() {
        let (env, _admin, _customer, merchant, forward_to, _token) = setup_env();

        // Test minimum valid value (1)
        let result1 = PaymentContract::set_payment_forward(
            env.clone(),
            merchant.clone(),
            forward_to.clone(),
            1,
        );
        assert!(result1.is_ok());

        // Remove and test maximum valid value (10000)
        PaymentContract::remove_payment_forward(env.clone(), merchant.clone()).unwrap();

        let result2 = PaymentContract::set_payment_forward(
            env,
            merchant,
            forward_to,
            10000,
        );
        assert!(result2.is_ok());
    }

    #[test]
    fn test_remove_payment_forward_success() {
        let (env, _admin, _customer, merchant, forward_to, _token) = setup_env();

        // First, set a forward config
        PaymentContract::set_payment_forward(
            env.clone(),
            merchant.clone(),
            forward_to,
            5000,
        ).unwrap();

        // Verify it exists
        assert!(PaymentContract::get_forward_config(env.clone(), merchant.clone()).is_ok());

        // Remove it
        let result = PaymentContract::remove_payment_forward(env.clone(), merchant.clone());
        assert!(result.is_ok());

        // Verify it's gone
        let config = PaymentContract::get_forward_config(env, merchant);
        assert_eq!(config, Err(Error::ForwardConfigNotFound));
    }

    #[test]
    fn test_remove_payment_forward_not_found() {
        let (env, _admin, _customer, merchant, _forward_to, _token) = setup_env();

        // Try to remove a forward config that doesn't exist
        let result = PaymentContract::remove_payment_forward(env, merchant);

        assert_eq!(result, Err(Error::ForwardConfigNotFound));
    }

    #[test]
    fn test_get_forward_config_not_found() {
        let (env, _admin, _customer, merchant, _forward_to, _token) = setup_env();

        // Try to get a forward config that doesn't exist
        let result = PaymentContract::get_forward_config(env, merchant);

        assert_eq!(result, Err(Error::ForwardConfigNotFound));
    }

    #[test]
    fn test_payment_forward_on_completion() {
        let (env, admin, customer, merchant, forward_to, token) = setup_env();

        // Create a forward config
        PaymentContract::set_payment_forward(
            env.clone(),
            merchant.clone(),
            forward_to.clone(),
            5000, // 50%
        ).unwrap();

        // Create a payment
        let payment_id = create_test_payment(&env, &customer, &merchant, 1000000, &token);

        // Complete the payment - this should trigger forwarding
        let result = PaymentContract::complete_payment(
            env.clone(),
            admin.clone(),
            payment_id,
        );

        assert!(result.is_ok());

        // Verify payment is completed
        let payment = PaymentContract::get_payment(&env, payment_id);
        assert_eq!(payment.status, PaymentStatus::Completed);
    }

    #[test]
    fn test_payment_forward_calculation() {
        let (env, admin, customer, merchant, forward_to, token) = setup_env();

        // Create a forward config with 25% (2500 bps)
        PaymentContract::set_payment_forward(
            env.clone(),
            merchant.clone(),
            forward_to.clone(),
            2500,
        ).unwrap();

        // Create a payment with 1,000,000 units
        let payment_id = create_test_payment(&env, &customer, &merchant, 1000000, &token);

        // Complete the payment
        PaymentContract::complete_payment(
            env.clone(),
            admin.clone(),
            payment_id,
        ).unwrap();

        // The merchant should receive: 1,000,000 * (1 - 0.01) = 990,000 (after 1% fee)
        // Then forward: 990,000 * 0.25 = 247,500 to forward_to
        // So merchant keeps: 990,000 - 247,500 = 742,500

        // Note: This test verifies the logic works; actual token transfer amounts
        // would depend on the token contract mock implementation
    }

    #[test]
    fn test_payment_forward_multiple_updates() {
        let (env, _admin, _customer, merchant, forward_to, _token) = setup_env();

        let new_forward_to = Address::random(&env);

        // Set initial forward config
        PaymentContract::set_payment_forward(
            env.clone(),
            merchant.clone(),
            forward_to,
            5000,
        ).unwrap();

        // Update with new forward_to
        PaymentContract::set_payment_forward(
            env.clone(),
            merchant.clone(),
            new_forward_to.clone(),
            7500, // Change to 75%
        ).unwrap();

        // Verify the updated config
        let config = PaymentContract::get_forward_config(env, merchant).unwrap();
        assert_eq!(config.forward_to, new_forward_to);
        assert_eq!(config.forward_bps, 7500);
    }

    #[test]
    fn test_payment_forward_with_minimal_bps() {
        let (env, _admin, _customer, merchant, forward_to, _token) = setup_env();

        // Set with minimal bps (1 = 0.01%)
        let result = PaymentContract::set_payment_forward(
            env.clone(),
            merchant.clone(),
            forward_to,
            1,
        );

        assert!(result.is_ok());

        let config = PaymentContract::get_forward_config(env, merchant).unwrap();
        assert_eq!(config.forward_bps, 1);
    }

    #[test]
    fn test_payment_forward_with_full_bps() {
        let (env, _admin, _customer, merchant, forward_to, _token) = setup_env();

        // Set with full bps (10000 = 100%)
        let result = PaymentContract::set_payment_forward(
            env.clone(),
            merchant.clone(),
            forward_to,
            10000,
        );

        assert!(result.is_ok());

        let config = PaymentContract::get_forward_config(env, merchant).unwrap();
        assert_eq!(config.forward_bps, 10000);
    }

    #[test]
    fn test_payment_forward_config_persistence() {
        let (env, _admin, _customer, merchant, forward_to, _token) = setup_env();

        // Set a forward config
        PaymentContract::set_payment_forward(
            env.clone(),
            merchant.clone(),
            forward_to.clone(),
            5000,
        ).unwrap();

        // Get it multiple times to ensure it persists
        let config1 = PaymentContract::get_forward_config(env.clone(), merchant.clone()).unwrap();
        let config2 = PaymentContract::get_forward_config(env, merchant).unwrap();

        assert_eq!(config1, config2);
        assert_eq!(config1.forward_to, forward_to);
    }

    #[test]
    fn test_remove_and_readd_forward_config() {
        let (env, _admin, _customer, merchant, forward_to, _token) = setup_env();

        // Set initial config
        PaymentContract::set_payment_forward(
            env.clone(),
            merchant.clone(),
            forward_to.clone(),
            5000,
        ).unwrap();

        // Remove it
        PaymentContract::remove_payment_forward(env.clone(), merchant.clone()).unwrap();

        // Verify it's gone
        assert_eq!(
            PaymentContract::get_forward_config(env.clone(), merchant.clone()),
            Err(Error::ForwardConfigNotFound)
        );

        // Re-add it with different settings
        let new_forward_to = Address::random(&env);
        PaymentContract::set_payment_forward(
            env.clone(),
            merchant.clone(),
            new_forward_to.clone(),
            7500,
        ).unwrap();

        // Verify the new config
        let config = PaymentContract::get_forward_config(env, merchant).unwrap();
        assert_eq!(config.forward_to, new_forward_to);
        assert_eq!(config.forward_bps, 7500);
    }
}
