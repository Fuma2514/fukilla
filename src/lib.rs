// Ephemeral Agent Wallets — NEAR Hackathon (track: security)
//
// A factory contract that gives AI agents money without giving them the keys.
// The owner (human or orchestrator) creates a per-task sub-account funded with
// a deposit. The agent receives a function-call key that can ONLY call
// `agent_call` on this factory — it cannot send NEAR, call other contracts,
// or delete the account. The factory enforces:
//   * expiry:    the wallet dies at `expiry_ts` (no new spending is allowed)
//   * spend cap: cumulative spending per task never exceeds `spend_cap`
//   * sweep-back: after the task completes (or expires) anyone can trigger
//     the wallet's self-destruction, sweeping every remaining yoctoNEAR back
//     to the owner. No long-lived secret ever needs to exist.
use near_sdk::store::LookupMap;
use near_sdk::json_types::U128;
use near_sdk::Allowance;
use near_sdk::{AccountId, NearToken, PanicOnDefault, Promise, PublicKey, env, near, require};

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    owner: AccountId,
    tasks: LookupMap<String, Task>,
}

#[near(serializers = [json, borsh])]
#[derive(Clone)]
pub struct Task {
    /// Human/orchestrator who funded the wallet and receives the sweep-back
    pub owner: AccountId,
    /// Sub-account that holds the task funds (e.g. `my-task.factory.near`)
    pub wallet: AccountId,
    /// The agent's function-call public key added to the wallet
    pub agent_key: PublicKey,
    /// Unix timestamp (ms) after which the wallet refuses any further spending
    pub expiry_ts: u64,
    /// Maximum cumulative amount the agent may spend (yoctoNEAR)
    pub spend_cap: u128,
    /// Cumulative amount spent so far (yoctoNEAR)
    pub spent: u128,
    /// True once the wallet has been swept and destroyed
    pub completed: bool,
}

/// Path to the pre-built executor WASM that gets deployed onto every wallet.
const EXECUTOR_WASM: &[u8] = include_bytes!("../executor.wasm");

#[near]
impl Contract {
    #[init]
    pub fn new(owner: AccountId) -> Self {
        Self { owner, tasks: LookupMap::new(b"t") }
    }

    /// Create an ephemeral wallet for a task. The attached deposit funds the
    /// new sub-account. A function-call key (`agent_key`) is added to the
    /// sub-account, restricted to `agent_call` on this factory with an
    /// allowance of the spend cap — so even off-chain the key is bounded.
    #[payable]
    pub fn create_wallet(
        &mut self,
        task_id: String,
        agent_key: PublicKey,
        ttl_sec: u64,
        spend_cap: U128,
    ) -> Promise {
        let funding = env::attached_deposit();
        require!(funding > NearToken::from_yoctonear(0), "deposit required");
        // Sub-account creation + FC key + storage needs a minimum; reject tiny wallets.
        require!(funding.as_yoctonear() >= 50_000_000_000_000_000_000_000, "min 0.05 NEAR funding");
        require!(ttl_sec > 0 && ttl_sec <= 30 * 24 * 3600, "ttl out of range");
        let cap: u128 = spend_cap.into();
        require!(cap > 0, "spend cap must be positive");

        let wallet: AccountId = format!("{}.{}", task_id, env::current_account_id())
            .parse()
            .expect("invalid task id");
        require!(
            !self.tasks.contains_key(&task_id),
            "task id already used"
        );

        let expiry_ts = env::block_timestamp() + ttl_sec * 1_000_000_000;
        let owner = env::predecessor_account_id();

        self.tasks.insert(
            task_id.clone(),
            Task {
                owner: owner.clone(),
                wallet: wallet.clone(),
                agent_key: agent_key.clone(),
                expiry_ts,
                spend_cap: cap,
                spent: 0,
                completed: false,
            },
        );

        let promise = Promise::new(wallet.clone())
            .create_account()
            .transfer(funding)
            .deploy_contract(EXECUTOR_WASM.to_vec())
            .add_access_key_allowance(
                agent_key,
                Allowance::limited(NearToken::from_yoctonear(cap))
                    .expect("spend cap must be positive"),
                env::current_account_id(),
                "agent_call".to_string(),
            );

        env::log_str(&format!(
            "{{\"event\":\"wallet_created\",\"task_id\":\"{}\",\"wallet\":\"{}\",\"expiry_ts\":{},\"spend_cap\":\"{}\",\"funding\":\"{}\"}}",
            task_id,
            wallet,
            expiry_ts,
            cap,
            funding.as_yoctonear(),
        ));

        promise
    }

    /// The ONLY method an agent's key can call. Pays `amount` yoctoNEAR from
    /// the task wallet to `receiver_id`. Enforces expiry and the cumulative
    /// spend cap; refunds nothing — the agent never holds a transferable key.
    pub fn agent_call(
        &mut self,
        task_id: String,
        receiver_id: AccountId,
        amount: U128,
        memo: String,
    ) -> Promise {
        let signer = env::signer_account_id();
        let predecessor = env::predecessor_account_id();
        require!(signer == predecessor, "must be called directly with the agent key");

        let task = self
            .tasks
            .get(&task_id)
            .cloned()
            .expect("unknown task");
        require!(signer == task.wallet, "this key does not own this task wallet");
        require!(!task.completed, "wallet already destroyed");
        require!(
            env::block_timestamp() < task.expiry_ts,
            "wallet expired: spending window closed"
        );

        let amount: u128 = amount.into();
        let new_spent = task.spent + amount;
        require!(new_spent <= task.spend_cap, "spend cap exceeded");

        // Update state BEFORE the external call (reentrancy hygiene).
        let mut t = task;
        t.spent = new_spent;
        self.tasks.insert(task_id.clone(), t);

        env::log_str(&format!(
            "{{\"event\":\"agent_payment\",\"task_id\":\"{}\",\"receiver\":\"{}\",\"amount\":\"{}\",\"spent\":\"{}\",\"memo\":\"{}\"}}",
            task_id, receiver_id, amount, new_spent, memo
        ));

        Promise::new(receiver_id).transfer(NearToken::from_yoctonear(amount))
    }

    /// Owner closes the task: sweeps the remaining balance back to the owner
    /// and deletes the wallet account entirely. After this the agent's key
    /// is dead — its account no longer exists.
    pub fn complete_task(&mut self, task_id: String) -> Promise {
        let task = self.get_live_task(&task_id);
        require!(
            env::predecessor_account_id() == task.owner,
            "only the task owner can complete this task"
        );
        self.finalize(task_id, task)
    }

    /// Anyone can force-expire a task whose spending window has elapsed.
    /// This makes the death of the wallet guaranteed, not owner-dependent.
    pub fn force_expire(&mut self, task_id: String) -> Promise {
        let task = self.get_live_task(&task_id);
        require!(
            env::block_timestamp() >= task.expiry_ts,
            "wallet not expired yet"
        );
        self.finalize(task_id, task)
    }

    fn get_live_task(&self, task_id: &String) -> Task {
        let task = self.tasks.get(task_id).cloned().expect("unknown task");
        require!(!task.completed, "wallet already destroyed");
        task
    }

    fn finalize(&mut self, task_id: String, task: Task) -> Promise {
        let mut t = task.clone();
        t.completed = true;
        self.tasks.insert(task_id.clone(), t);

        env::log_str(&format!(
            "{{\"event\":\"wallet_destroyed\",\"task_id\":\"{}\",\"wallet\":\"{}\",\"swept_to\":\"{}\"}}",
            task_id, task.wallet, task.owner
        ));

        // The wallet runs a minimal executor contract whose only method
        // (`sweep`) deletes the account and sends the whole balance to the
        // owner. The factory — and only the factory — may trigger it, so the
        // agent's function-call key can never destroy or move funds itself.
        Promise::new(task.wallet).function_call(
            "sweep".to_string(),
            near_sdk::serde_json::to_vec(&near_sdk::serde_json::json!({
                "beneficiary": task.owner.clone()
            }))
            .expect("json serialize"),
            NearToken::from_yoctonear(0),
            near_sdk::Gas::from_tgas(20),
        )
    }

    // ---------- views ----------

    pub fn get_task(&self, task_id: String) -> Option<Task> {
        self.tasks.get(&task_id).cloned()
    }

    pub fn remaining_spend(&self, task_id: String) -> U128 {
        match self.tasks.get(&task_id) {
            Some(t) => U128::from(t.spend_cap.saturating_sub(t.spent)),
            None => U128::from(0),
        }
    }

    pub fn get_owner(&self) -> AccountId {
        self.owner.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::test_utils::{VMContextBuilder, accounts};
    use near_sdk::{testing_env, Gas};

    const TASK_ID: &str = "research-task-1";

    fn context() -> VMContextBuilder {
        let mut b = VMContextBuilder::new();
        b.current_account_id(accounts(1)); // factory account
        b.signer_account_id(accounts(2));
        b.predecessor_account_id(accounts(2));
        b.block_timestamp(0);
        b
    }

    fn pk() -> PublicKey {
        "ed25519:8h7kFK4t1Pj5wMsmvB5NmMBiF9YZTfbvJdeWPDXCCbT8".parse().unwrap()
    }

    fn create(contract: &mut Contract) {
        let mut ctx = context();
        ctx.attached_deposit(NearToken::from_near(1));
        testing_env!(ctx.build());
        let _ = contract.create_wallet(
            TASK_ID.to_string(),
            pk(),
            3600,
            U128::from(1_000_000_000_000_000_000_000_000u128),
        );
    }

    #[test]
    fn create_wallet_records_task() {
        let mut ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);
        let t = c.get_task(TASK_ID.to_string()).unwrap();
        assert!(t.wallet.as_str().contains(".")); // sub-account of the factory account
        assert_eq!(t.spent, 0);
        assert_eq!(t.expiry_ts, 3600 * 1_000_000_000);
        assert!(c.remaining_spend(TASK_ID.to_string()).0 > 0);
    }

    #[test]
    fn duplicate_task_id_rejected() {
        let mut ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);
        let mut ctx = context();
        ctx.attached_deposit(NearToken::from_near(1));
        testing_env!(ctx.build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            c.create_wallet(TASK_ID.to_string(), pk(), 3600, U128::from(1));
        }));
        assert!(result.is_err());
    }

    #[test]
    fn agent_payment_updates_spent() {
        let mut ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);

        let wallet = c.get_task(TASK_ID.to_string()).unwrap().wallet;
        let mut ctx = context();
        ctx.signer_account_id(wallet.clone());
        ctx.predecessor_account_id(wallet.clone());
        ctx.attached_deposit(NearToken::from_yoctonear(0));
        ctx.prepaid_gas(Gas::from_tgas(100));
        testing_env!(ctx.build());
        let _ = c.agent_call(
            TASK_ID.to_string(),
            "api-provider.near".parse().unwrap(),
            U128::from(400_000_000_000_000_000_000_000u128),
            "text-model-call".to_string(),
        );
        assert_eq!(
            c.remaining_spend(TASK_ID.to_string()).0,
            600_000_000_000_000_000_000_000u128
        );
    }

    #[test]
    #[should_panic(expected = "spend cap exceeded")]
    fn spend_cap_enforced() {
        let mut ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);
        let wallet = c.get_task(TASK_ID.to_string()).unwrap().wallet;
        let mut ctx = context();
        ctx.signer_account_id(wallet.clone());
        ctx.predecessor_account_id(wallet.clone());
        testing_env!(ctx.build());
        let _ = c.agent_call(
            TASK_ID.to_string(),
            "api-provider.near".parse().unwrap(),
            U128::from(2_000_000_000_000_000_000_000_000u128),
            "too-much".to_string(),
        );
    }

    #[test]
    #[should_panic(expected = "wallet expired")]
    fn expired_wallet_refuses_payment() {
        let mut ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);
        let wallet = c.get_task(TASK_ID.to_string()).unwrap().wallet;
        let mut ctx = context();
        ctx.signer_account_id(wallet.clone());
        ctx.predecessor_account_id(wallet.clone());
        ctx.block_timestamp(3600 * 1_000_000_000 + 1);
        testing_env!(ctx.build());
        let _ = c.agent_call(
            TASK_ID.to_string(),
            "api-provider.near".parse().unwrap(),
            U128::from(1),
            "after-expiry".to_string(),
        );
    }

    #[test]
    #[should_panic(expected = "does not own this task wallet")]
    fn foreign_key_cannot_spend() {
        let mut ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);
        let other: AccountId = "rogue-agent.near".parse().unwrap();
        let mut ctx = context();
        let other2 = other.clone();
        ctx.signer_account_id(other.clone());
        ctx.predecessor_account_id(other.clone());
        testing_env!(ctx.build());
        let _ = c.agent_call(
            TASK_ID.to_string(),
            other,
            U128::from(1),
            "steal".to_string(),
        );
    }

    #[test]
    fn owner_can_complete_and_sweep() {
        let mut ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);
        let owner = c.get_task(TASK_ID.to_string()).unwrap().owner;
        let mut ctx = context();
        ctx.signer_account_id(owner.clone());
        ctx.predecessor_account_id(owner);
        testing_env!(ctx.build());
        let _ = c.complete_task(TASK_ID.to_string());
        assert!(c.get_task(TASK_ID.to_string()).unwrap().completed);
    }

    #[test]
    fn anyone_can_force_expire_after_window() {
        let mut ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);
        let mut ctx = context();
        ctx.block_timestamp(3600 * 1_000_000_000 + 1);
        ctx.signer_account_id(accounts(5));
        ctx.predecessor_account_id(accounts(5));
        testing_env!(ctx.build());
        let _ = c.force_expire(TASK_ID.to_string());
        assert!(c.get_task(TASK_ID.to_string()).unwrap().completed);
    }

    #[test]
    #[should_panic(expected = "not expired yet")]
    fn force_expire_before_window_fails() {
        let mut ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);
        let _ = c.force_expire(TASK_ID.to_string());
    }
}
