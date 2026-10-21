use super::*;

/// Builds an escrow that is fully funded to its `funding_target`, so `status == 1`
/// (Funded). `withdraw()` requires `EscrowStatus::Funded`; withdrawing while the
/// escrow is still open (status 0) reverts with `WithdrawalNotFunded`.
///
/// A real Stellar Asset Contract is used because `withdraw()` transfers the
/// released principal to the SME and therefore needs an actual token balance on
/// the contract.
fn funded_client(env: &Env) -> (StarfundEscrowClient<'_>, Address, Address) {
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(env));
    let token = sac.address();
    let sac_admin = StellarAssetClient::new(env, &token);

    let (client, admin, sme) = setup(env);
    let (_, treasury) = free_addresses(env);
    client.init(
        &admin,
        &String::from_str(env, "DISPUTE001"),
        &sme,
        &100_000_000_000i128,
        &100i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );
    let investor = Address::generate(env);
    sac_admin.mint(&investor, &100_000_000_000i128);
    // Funding the full `funding_target` moves the escrow to status 1 (Funded).
    client.fund(&investor, &100_000_000_000i128);
    sac_admin.mint(&client.address, &100_000_000_000i128);
    (client, admin, sme)
}

#[test]
fn release_before_dispute_succeeds() {
    let env = Env::default();
    let (client, _, _) = funded_client(&env);
    let before = client.get_escrow();
    // `withdraw()` is gated on `EscrowStatus::Funded`; asserting status 1 here
    // documents the precondition the release path depends on.
    assert_eq!(before.status, 1);
    assert!(!before.dispute_active);

    let released = client.withdraw();
    assert_eq!(released.status, 3);
    assert!(!client.is_dispute_active());
}

#[test]
fn release_before_funding_is_rejected() {
    // Status 0 (open) is not withdrawable: `withdraw()` requires status 1.
    let env = Env::default();
    env.mock_all_auths();
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &token);
    let (client, admin, sme) = setup(&env);
    let (_, treasury) = free_addresses(&env);
    client.init(
        &admin,
        &String::from_str(&env, "DISPUTE000"),
        &sme,
        &100_000_000_000i128,
        &100i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );
    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &90_000_000_000i128);
    client.fund(&investor, &90_000_000_000i128);
    assert_eq!(client.get_escrow().status, 0);

    let result = client.try_withdraw();
    assert_contract_error(result, EscrowError::WithdrawalNotFunded);
}

#[test]
fn release_during_dispute_is_blocked() {
    let env = Env::default();
    let (client, admin, _) = funded_client(&env);
    client.open_dispute(&admin);
    assert!(client.is_dispute_active());
    assert!(client.get_escrow().dispute_active);

    let result = client.try_withdraw();
    assert_contract_error(result, EscrowError::DisputeBlocksWithdrawal);
}

#[test]
fn close_during_dispute_is_blocked() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    client.init(
        &admin,
        &String::from_str(&env, "CLOSE-DISPUTE"),
        &sme,
        &1000i128,
        &100i64,
        &0u64,
        &Address::generate(&env),
        &None,
        &Address::generate(&env),
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );
    client.open_dispute(&admin);

    let result = client.try_close_escrow();
    match result {
        Err(Err(InvokeError::Contract(code))) => {
            assert_eq!(code, crate::CloseError::ActiveDispute as u32);
        }
        other => panic!("expected active dispute error, got {other:?}"),
    }
}

#[test]
fn dispute_opened_during_release_flow_blocks_release() {
    let env = Env::default();
    let (client, admin, _) = funded_client(&env);
    client.open_dispute(&admin);
    let result = client.try_withdraw();
    assert_contract_error(result, EscrowError::DisputeBlocksWithdrawal);
}

#[test]
fn dispute_resolved_then_release_succeeds() {
    let env = Env::default();
    let (client, admin, _) = funded_client(&env);
    client.open_dispute(&admin);
    client.close_dispute(&admin, &true);

    let released = client.withdraw();
    assert_eq!(released.status, 3);
    assert!(!client.is_dispute_active());
}

#[test]
fn rejected_dispute_resolution_is_explicit_and_non_mutating() {
    let env = Env::default();
    let (client, admin, _) = funded_client(&env);
    client.open_dispute(&admin);
    let before = client.get_dispute_record().unwrap();

    let result = client.try_close_dispute(&admin, &false);
    assert_contract_error(result, EscrowError::DisputeResolutionRejected);

    assert!(client.is_dispute_active());
    assert_eq!(client.get_escrow().dispute_active, true);
    assert_eq!(client.get_dispute_record().unwrap(), before);
}

#[test]
fn set_dispute_active_resolves_through_dispute_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    let (_, treasury) = free_addresses(&env);
    let sac = env.register_stellar_asset_contract_v2(Address::generate(&env));
    let token = sac.address();
    let sac_admin = StellarAssetClient::new(&env, &token);

    client.init(
        &admin,
        &String::from_str(&env, "DISPUTE-LC"),
        &sme,
        &100_000_000_000i128,
        &100i64,
        &0u64,
        &token,
        &None,
        &treasury,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None,
        &None::<i64>,
        &None::<u32>,
    );
    let investor = Address::generate(&env);
    sac_admin.mint(&investor, &100_000_000_000i128);
    client.fund(&investor, &100_000_000_000i128);
    sac_admin.mint(&client.address, &100_000_000_000i128);

    client.set_dispute_active(&true);
    assert!(client.is_dispute_active());
    assert!(client.get_escrow().dispute_active);
    assert_eq!(
        client.get_dispute_record().unwrap().state,
        crate::DisputeState::Open
    );

    let events_before = env.events().all().events().len();
    client.set_dispute_active(&false);

    let record = client.get_dispute_record().unwrap();
    assert_eq!(record.state, crate::DisputeState::Resolved);
    assert!(record.resolved_at.is_some());
    assert!(record.resolved_by.is_some());
    assert!(!client.is_dispute_active());
    assert!(!client.get_escrow().dispute_active);
    assert_eq!(env.events().all().events().len(), events_before + 1);
}

#[test]
fn unauthorized_dispute_close_panics() {
    let env = Env::default();
    let (client, _, _) = funded_client(&env);
    let outsider = Address::generate(&env);
    let err = client.try_close_dispute(&outsider, &true);
    assert_contract_error(err, EscrowError::Unauthorized);
}

#[test]
fn cancel_funding_during_dispute_is_blocked() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, sme) = setup(&env);
    default_init(&client, &env, &admin, &sme);

    client.open_dispute(&admin);
    assert!(client.is_dispute_active());

    let result = client.try_cancel_funding(&0u32);
    assert_contract_error(result, EscrowError::DisputeBlocksCancelFunding);
}
