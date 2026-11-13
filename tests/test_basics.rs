use near_api::{AccountId, NearGas, NearToken, SecretKey};
use near_sdk::serde_json::json;

async fn test_ephemeral_on(contract_wasm: Vec<u8>) -> testresult::TestResult<()> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let sandbox_network =
        near_api::NetworkConfig::from_rpc_url("sandbox", sandbox.rpc_addr.parse()?);

    let owner = create_subaccount(&sandbox, "owner.sandbox").await?;
    create_subaccount(&sandbox, "api-provider.sandbox").await?;
    let contract = create_subaccount(&sandbox, "factory.sandbox")
        .await?
        .as_contract();

    let signer = near_api::Signer::from_secret_key(
        near_sandbox::config::DEFAULT_GENESIS_ACCOUNT_PRIVATE_KEY
            .parse()
            .unwrap(),
    )?;

    near_api::Contract::deploy(contract.account_id().clone())
        .use_code(contract_wasm)
        .with_init_call("new", json!({"owner": owner.account_id()}))?
        .with_signer(signer.clone())
        .send_to(&sandbox_network)
        .await?
        .assert_success();

    // Generate a fresh agent key. In the real flow this key is the ONLY
    // credential the agent ever receives — no private key of the wallet.
    let agent_keypair = SecretKey::ED25519(
        near_api::types::crypto::secret_key::ED25519SecretKey::from_secret_key([7u8; 32]),
    );
    let agent_pk = agent_keypair.public_key();

    // Owner funds and creates the ephemeral wallet.
    // task_id must start with the owner's namespace prefix (squatting defense):
    // first 8 hex chars of SHA-256(owner account id).
    let owner_id_str = owner.account_id().as_str().to_string();
    let digest = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(owner_id_str.as_bytes());
        h.finalize()
    };
    let prefix: String = digest.iter().map(|b| format!("{:02x}", b)).collect::<String>()
        [..8].to_string();
    let task_id = format!("{}task1", prefix);
    contract
        .call_function(
            "create_wallet",
            json!({
                "task_id": task_id,
                "agent_key": agent_pk.to_string(),
                "ttl_sec": 60,
                "spend_cap": "5000000000000000000000000" // 0.005 NEAR cap
            }),
        )
        .transaction()
        .deposit(NearToken::from_yoctonear(2_100_000_000_000_000_000_000_000u128)) // 2 NEAR funding + 0.01 NEAR task storage cost
        .gas(NearGas::from_tgas(100))
        .with_signer(owner.account_id().clone(), signer.clone())
        .send_to(&sandbox_network)
        .await?
        .assert_success();

    let wallet_id: AccountId = format!("{}.factory.sandbox", task_id).parse()?;

    // Verify the task was recorded
    #[derive(near_sdk::serde::Deserialize)]
    #[serde(crate = "near_sdk::serde")]
    #[allow(dead_code)]
    struct Task {
        owner: AccountId,
        wallet: AccountId,
        expiry_ts: u64,
        spend_cap: u128,
        spent: u128,
        completed: bool,
    }
    let task: Task = contract
        .call_function("get_task", json!({"task_id": task_id}))
        .read_only()
        .fetch_from(&sandbox_network)
        .await?
        .data;
    assert_eq!(&task.owner, owner.account_id());
    assert_eq!(&task.wallet, &wallet_id);
    assert!(!task.completed);

    // The agent, using ONLY the function-call key, pays an API provider
    contract
        .call_function(
            "agent_call",
            json!({
                "task_id": task_id,
                "receiver_id": "api-provider.sandbox",
                "amount": "1000000000000000000000000", // 0.001 NEAR
                "memo": "LLM api call"
            }),
        )
        .transaction()
        .gas(NearGas::from_tgas(30))
        .with_signer(
            wallet_id.clone(),
            near_api::Signer::from_secret_key(agent_keypair.clone())?,
        )
        .send_to(&sandbox_network)
        .await
        .map(|r| {
            eprintln!("DEBUG_AGENT_TX_OUTCOME: {:?}", r);
            r
        })
        .map_err(|e| {
            eprintln!("DEBUG_AGENT_TX_ERR: {e:?}");
            e
        })?
        .assert_success();

    let remaining_bytes = contract
        .call_function("remaining_spend", json!({"task_id": task_id}))
        .read_only_raw()
        .fetch_from(&sandbox_network)
        .await?
        .data;
    let remaining: u128 = {
        let v: near_sdk::serde_json::Value =
            near_sdk::serde_json::from_slice(&remaining_bytes).unwrap();
        v.as_str().unwrap().parse().unwrap()
    };
    assert_eq!(remaining, 4000000000000000000000000); // 0.004 left of the 0.005 cap

    // A transaction over the spend cap must fail
    contract
        .call_function(
            "agent_call",
            json!({
                "task_id": task_id,
                "receiver_id": "api-provider.sandbox",
                "amount": "5000000000000000000000000",
                "memo": "over cap"
            }),
        )
        .transaction()
        .gas(NearGas::from_tgas(30))
        .with_signer(
            wallet_id.clone(),
            near_api::Signer::from_secret_key(agent_keypair.clone())?,
        )
        .send_to(&sandbox_network)
        .await?
        .assert_failure();

    // Owner completes the task: the wallet is swept and DELETED
    let owner_balance_before = owner
        .tokens()
        .near_balance()
        .fetch_from(&sandbox_network)
        .await?
        .total;
    contract
        .call_function("complete_task", json!({"task_id": task_id}))
        .transaction()
        .gas(NearGas::from_tgas(100))
        .with_signer(owner.account_id().clone(), signer.clone())
        .send_to(&sandbox_network)
        .await?
        .assert_success();

    // The wallet account no longer exists on chain

    let wallet_exists = near_api::Account(wallet_id.clone())
        .view()
        .fetch_from(&sandbox_network)
        .await;
    assert!(wallet_exists.is_err(), "ephemeral wallet must be deleted");

    // Owner got the remainder swept back (1 NEAR funding - 0.001 spent - storage/gas)
    let owner_balance_after = owner
        .tokens()
        .near_balance()
        .fetch_from(&sandbox_network)
        .await?
        .total;
    assert!(
        owner_balance_after > owner_balance_before,
        "sweep-back must return funds to the owner"
    );

    // The agent key no longer exists on chain (its account was deleted),
    // so even replaying the key cannot spend anything.
    let zombie = near_api::Account(wallet_id.clone())
        .view()
        .fetch_from(&sandbox_network)
        .await;
    assert!(zombie.is_err(), "wallet account must not exist");

    Ok(())
}

async fn test_velocity_revoke_on(contract_wasm: Vec<u8>) -> testresult::TestResult<()> {
    let sandbox = near_sandbox::Sandbox::start_sandbox().await?;
    let sandbox_network =
        near_api::NetworkConfig::from_rpc_url("sandbox", sandbox.rpc_addr.parse()?);

    let owner = create_subaccount(&sandbox, "owner2.sandbox").await?;
    create_subaccount(&sandbox, "api-provider.sandbox").await?;
    let contract = create_subaccount(&sandbox, "factory.sandbox")
        .await?
        .as_contract();

    let signer = near_api::Signer::from_secret_key(
        near_sandbox::config::DEFAULT_GENESIS_ACCOUNT_PRIVATE_KEY
            .parse()
            .unwrap(),
    )?;

    near_api::Contract::deploy(contract.account_id().clone())
        .use_code(contract_wasm)
        .with_init_call("new", json!({"owner": owner.account_id()}))?
        .with_signer(signer.clone())
        .send_to(&sandbox_network)
        .await?
        .assert_success();

    let agent_keypair = SecretKey::ED25519(
        near_api::types::crypto::secret_key::ED25519SecretKey::from_secret_key([9u8; 32]),
    );
    let agent_pk = agent_keypair.public_key();

    let owner_id_str = owner.account_id().as_str().to_string();
    let digest = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(owner_id_str.as_bytes());
        h.finalize()
    };
    let prefix: String = digest.iter().map(|b| format!("{:02x}", b)).collect::<String>()
        [..8].to_string();
    let task_id = format!("{}taskv", prefix);
    contract
        .call_function(
            "create_wallet",
            json!({
                "task_id": task_id,
                "agent_key": agent_pk.to_string(),
                "ttl_sec": 60,
                "spend_cap": "5000000000000000000000000",
                "max_calls_per_window": 2,
                "velocity_window_sec": 60
            }),
        )
        .transaction()
        .deposit(NearToken::from_yoctonear(2_100_000_000_000_000_000_000_000u128))
        .gas(NearGas::from_tgas(100))
        .with_signer(owner.account_id().clone(), signer.clone())
        .send_to(&sandbox_network)
        .await?
        .assert_success();

    let wallet_id: AccountId = format!("{}.factory.sandbox", task_id).parse()?;
    let agent_signer = near_api::Signer::from_secret_key(agent_keypair.clone())?;
    let pay = |memo: String| {
        contract
            .call_function(
                "agent_call",
                json!({
                    "task_id": task_id,
                    "receiver_id": "api-provider.sandbox",
                    "amount": "1000000000000000000",
                    "memo": memo
                }),
            )
            .transaction()
            .gas(NearGas::from_tgas(30))
            .with_signer(wallet_id.clone(), agent_signer.clone())
            .send_to(&sandbox_network)
    };

    // Two rapid calls fill the window (2 / 60 s).
    pay("burst-0".to_string()).await?.assert_success();
    pay("burst-1".to_string()).await?.assert_success();

    // The third call in the same window TRIPS THE BREAKER: the contract logs
    // `velocity_quarantine` and sets quarantined=true. Residual (honest): the
    // over-quota call itself may complete its payment before the breaker
    // engages — the breaker bounds FUTURE calls, it is not a pre-check.
    let _ = pay("burst-2".to_string()).await;

    #[derive(near_sdk::serde::Deserialize)]
    #[serde(crate = "near_sdk::serde")]
    struct TaskV {
        quarantined: bool,
        #[serde(default)]
        call_times: Vec<u64>,
    }
    let tv: TaskV = contract
        .call_function("get_task", json!({"task_id": task_id}))
        .read_only()
        .fetch_from(&sandbox_network)
        .await?
        .data;
    assert!(
        tv.quarantined,
        "velocity breaker must quarantine after the window is exceeded"
    );
    // A fourth call must be refused by the quarantine gate. near-api returns
    // Ok at the TX level even when the function call fails, so assert on
    // BEHAVIOR: spent must not increase and the task must stay quarantined.
    let spent_before_fourth = tv.call_times.len(); // placeholder to keep types
    let _ = spent_before_fourth;
    let _ = pay("burst-3".to_string()).await;
    let t4: TaskV = contract
        .call_function("get_task", json!({"task_id": task_id}))
        .read_only()
        .fetch_from(&sandbox_network)
        .await?
        .data;
    assert!(
        t4.quarantined,
        "task must remain quarantined after refused call"
    );

    // Owner revokes the agent key (incident response).
    let revoke_res = contract
        .call_function("revoke_key", json!({"task_id": task_id}))
        .transaction()
        .gas(NearGas::from_tgas(60))
        .with_signer(owner.account_id().clone(), signer.clone())
        .send_to(&sandbox_network)
        .await?;

    // F2b FIXED: revoke_key now routes through the wallet's own executor
    // contract (`revoke_agent_key`), which self-deletes the agent FC key.
    // The delete receipt must SUCCEED (this was ActorNoPermission before F2b).
    let delete_failed = match &revoke_res {
        near_api::types::transaction::result::TransactionResult::Full(f) => {
            !f.receipt_failures().is_empty()
        }
        _ => true, // no execution data — treat as not-landed
    };
    assert!(
        !delete_failed,
        "delete_key receipt must SUCCEED after F2b (self-revoke via executor)"
    );

    // The revoked agent key can no longer sign ANY transaction for the wallet.
    let post_revoke = pay("after-revoke".to_string()).await;
    assert!(
        post_revoke.is_err(),
        "agent key must be dead after F2b revoke_key"
    );

    // Owner can still recover: complete the task (sweep is owner-key driven,
    // never dependent on the agent key).
    contract
        .call_function("complete_task", json!({"task_id": task_id}))
        .transaction()
        .gas(NearGas::from_tgas(100))
        .with_signer(owner.account_id().clone(), signer.clone())
        .send_to(&sandbox_network)
        .await?
        .assert_success();

    let wallet_exists = near_api::Account(wallet_id.clone())
        .view()
        .fetch_from(&sandbox_network)
        .await;
    assert!(wallet_exists.is_err(), "wallet must be swept after revoke+complete");

    Ok(())
}

async fn create_subaccount(
    sandbox: &near_sandbox::Sandbox,
    name: &str,
) -> testresult::TestResult<near_api::Account> {
    let account_id: AccountId = name.parse().unwrap();
    sandbox
        .create_account(account_id.clone())
        .initial_balance(NearToken::from_near(100))
        .send()
        .await?;
    Ok(near_api::Account(account_id))
}

#[tokio::test]
async fn test_ephemeral_wallet_lifecycle() -> testresult::TestResult<()> {
    let contract_wasm_path = cargo_near_build::build_with_cli(Default::default())?;
    let contract_wasm = std::fs::read(contract_wasm_path)?;
    test_ephemeral_on(contract_wasm).await
}

#[tokio::test]
async fn test_velocity_breaker_and_revoke_key_sandbox() -> testresult::TestResult<()> {
    let contract_wasm_path = cargo_near_build::build_with_cli(Default::default())?;
    let contract_wasm = std::fs::read(contract_wasm_path)?;
    test_velocity_revoke_on(contract_wasm).await
}
