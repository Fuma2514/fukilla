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
use near_sdk::Allowance;
use near_sdk::json_types::U128;
use near_sdk::store::LookupMap;
use near_sdk::{AccountId, NearToken, PanicOnDefault, Promise, PublicKey, env, near, require};

#[near(contract_state)]
#[derive(PanicOnDefault)]
pub struct Contract {
    owner: AccountId,
    tasks: LookupMap<String, Task>,
    /// Owner-configured canary bait account: if the agent ever "pays" this
    /// account, the transfer is silently converted into a sweep-back and the
    /// wallet is locked forever. Protocol-level honeypot — the canary never
    /// receives funds, and the "successful payment" is the alarm itself.
    canary: Option<AccountId>,
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
    // ---- SENTINEL layers (agent-level defense, on-chain) ----
    /// Max payments allowed inside the velocity window (0 = disabled)
    pub max_calls_per_window: u32,
    /// Velocity window length in nanoseconds
    pub velocity_window_ns: u64,
    /// Timestamps (ns) of payments inside the current velocity window
    pub call_times: Vec<u64>,
    /// True once the wallet has quarantined itself (locked, awaiting owner)
    pub quarantined: bool,
}

/// Path to the pre-built executor WASM that gets deployed onto every wallet.
const EXECUTOR_WASM: &[u8] = include_bytes!("../executor.wasm");

#[near]
impl Contract {
    #[init]
    pub fn new(owner: AccountId) -> Self {
        Self {
            owner,
            tasks: LookupMap::new(b"t"),
            canary: None,
        }
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
        max_calls_per_window: Option<u32>,
        velocity_window_sec: Option<u64>,
    ) -> Promise {
        let funding = env::attached_deposit();
        require!(funding > NearToken::from_yoctonear(0), "deposit required");
        // Sub-account creation + FC key + storage needs a minimum; reject tiny wallets.
        require!(
            funding.as_yoctonear() >= 50_000_000_000_000_000_000_000,
            "min 0.05 NEAR funding"
        );
        require!(ttl_sec > 0 && ttl_sec <= 30 * 24 * 3600, "ttl out of range");
        let cap: u128 = spend_cap.into();
        require!(cap > 0, "spend cap must be positive");
        // SENTINEL sanity caps: prevent unbounded velocity bookkeeping (gas
        // griefing) and u64 overflow when converting the window to nanoseconds.
        if let Some(n) = max_calls_per_window {
            require!(
                n > 0 && n <= 10_000,
                "max_calls_per_window must be 1..=10000"
            );
        }
        if let Some(w) = velocity_window_sec {
            require!(
                w > 0 && w <= 30 * 24 * 3600,
                "velocity window must be 1s..=30d"
            );
        }
        // Validate task_id so the derived sub-account is a valid DIRECT sub-account.
        // A task_id containing dots would parse into a nested account that the
        // factory cannot create -> promise failure -> funding stuck + registry
        // entry burned forever.
        require!(
            !task_id.is_empty()
                && task_id.len() <= 32
                && task_id
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
                && !task_id.starts_with('-')
                && !task_id.ends_with('-'),
            "task_id must be 1-32 chars: a-z, 0-9, '-', '_'"
        );

        let wallet: AccountId = format!("{}.{}", task_id, env::current_account_id())
            .parse()
            .expect("invalid task id");
        require!(!self.tasks.contains_key(&task_id), "task id already used");

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
                max_calls_per_window: max_calls_per_window.unwrap_or(0),
                velocity_window_ns: velocity_window_sec.unwrap_or(0) * 1_000_000_000,
                call_times: Vec::new(),
                quarantined: false,
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
        require!(
            signer == predecessor,
            "must be called directly with the agent key"
        );
        // Reject stray deposits: an FC key CAN attach deposits (bounded by allowance)
        // and any attached deposit would be silently stranded in the factory.
        require!(
            env::attached_deposit() == NearToken::from_yoctonear(0),
            "agent_call must not attach a deposit"
        );

        let task = self.tasks.get(&task_id).cloned().expect("unknown task");
        require!(
            signer == task.wallet,
            "this key does not own this task wallet"
        );
        require!(!task.completed, "wallet already destroyed");
        // ---- LAYER: quarantine gate ---- a quarantined wallet accepts nothing.
        require!(!task.quarantined, "wallet quarantined: behavior flagged");
        require!(
            env::block_timestamp() < task.expiry_ts,
            "wallet expired: spending window closed"
        );

        // ---- LAYER: velocity circuit breaker (on-chain self-quarantine) ----
        // A violating call does NOT panic (a panic rolls back and destroys the
        // evidence). Instead the transaction SUCCEEDS without paying anything
        // and the wallet locks itself forever: permanent, unerasable on-chain
        // proof that the agent attempted anomalous behavior.
        let mut t = task;
        if t.max_calls_per_window > 0 {
            let now = env::block_timestamp();
            let window_ns = t.velocity_window_ns;
            let mut recent: Vec<u64> = t
                .call_times
                .iter()
                .copied()
                .filter(|ts| now.saturating_sub(*ts) < window_ns)
                .collect();
            if recent.len() >= t.max_calls_per_window as usize {
                t.quarantined = true;
                t.call_times = recent;
                self.tasks.insert(task_id.clone(), t.clone());
                env::log_str(&format!(
                    "{{\"event\":\"velocity_quarantine\",\"task_id\":\"{}\",\"attempts_in_window\":\"{}\"}}",
                    task_id, t.max_calls_per_window
                ));
                return Promise::new(t.owner.clone()).transfer(NearToken::from_yoctonear(0));
            }
            recent.push(now);
            t.call_times = recent;
        }

        let amount: u128 = amount.into();
        let new_spent = t.spent + amount;
        require!(new_spent <= t.spend_cap, "spend cap exceeded");

        // ---- LAYER: canary honeypot (protocol-level bait) ----
        // If the agent ever "pays" the canary, no funds ever reach the bait:
        // the wallet locks itself permanently and everything sweeps back to
        // the owner. A "successful" canary payment IS the alarm.
        if self.canary == Some(receiver_id.clone()) {
            t.spent = new_spent;
            t.quarantined = true;
            t.completed = true;
            env::log_str(&format!(
                "{{\"event\":\"canary_triggered\",\"task_id\":\"{}\",\"receiver\":\"{}\"}}",
                task_id, receiver_id
            ));
            return self.finalize(task_id, t);
        }

        // Update state BEFORE the external call (reentrancy hygiene).
        t.spent = new_spent;
        self.tasks.insert(task_id.clone(), t);

        env::log_str(&format!(
            "{{\"event\":\"agent_payment\",\"task_id\":\"{}\",\"receiver\":\"{}\",\"amount\":\"{}\",\"spent\":\"{}\",\"memo\":\"{}\"}}",
            task_id, receiver_id, amount, new_spent, memo
        ));

        Promise::new(receiver_id).transfer(NearToken::from_yoctonear(amount))
    }

    /// Owner sets the canary bait account (honeypot). Only the factory owner.
    pub fn set_canary(&mut self, canary: AccountId) {
        require!(
            env::predecessor_account_id() == self.owner,
            "only the factory owner can set the canary"
        );
        self.canary = Some(canary);
    }

    /// Owner clears the quarantine after reviewing the incident.
    pub fn lift_quarantine(&mut self, task_id: String) {
        let mut task = self.tasks.get(&task_id).cloned().expect("unknown task");
        require!(
            env::predecessor_account_id() == task.owner,
            "only the task owner can lift the quarantine"
        );
        require!(task.quarantined, "wallet not quarantined");
        task.quarantined = false;
        task.call_times = Vec::new();
        self.tasks.insert(task_id, task);
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
    use near_sdk::{Gas, testing_env};

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
        "ed25519:8h7kFK4t1Pj5wMsmvB5NmMBiF9YZTfbvJdeWPDXCCbT8"
            .parse()
            .unwrap()
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
            Some(30),
            Some(3600),
        );
    }

    #[test]
    fn create_wallet_records_task() {
        let ctx = context();
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
        let ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);
        let mut ctx = context();
        ctx.attached_deposit(NearToken::from_near(1));
        testing_env!(ctx.build());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = c.create_wallet(TASK_ID.to_string(), pk(), 3600, U128::from(1), None, None);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn agent_payment_updates_spent() {
        let ctx = context();
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
        let ctx = context();
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
        let ctx = context();
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
        let ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);
        let other: AccountId = "rogue-agent.near".parse().unwrap();
        let mut ctx = context();
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
        let ctx = context();
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
        let ctx = context();
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
        let ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);
        let _ = c.force_expire(TASK_ID.to_string());
    }

    #[test]
    #[should_panic(expected = "task_id must be")]
    fn dotted_task_id_rejected() {
        let mut ctx = context();
        ctx.attached_deposit(NearToken::from_near(1));
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        let _ = c.create_wallet(
            "bad.dotted".to_string(),
            pk(),
            3600,
            U128::from(1_000_000_000_000_000_000_000_000u128),
            None,
            None,
        );
    }

    #[test]
    #[should_panic(expected = "agent_call must not attach")]
    fn agent_call_rejects_deposit() {
        let ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        create(&mut c);
        let wallet = c.get_task(TASK_ID.to_string()).unwrap().wallet;
        let mut ctx = context();
        ctx.signer_account_id(wallet.clone());
        ctx.predecessor_account_id(wallet.clone());
        ctx.attached_deposit(NearToken::from_yoctonear(1));
        ctx.prepaid_gas(Gas::from_tgas(100));
        testing_env!(ctx.build());
        let _ = c.agent_call(
            TASK_ID.to_string(),
            "api-provider.near".parse().unwrap(),
            U128::from(1),
            "with-deposit".to_string(),
        );
    }

    fn sentinel_wallet(c: &mut Contract) {
        // wallet with velocity limit 2 calls / 60s
        let mut ctx = context();
        ctx.attached_deposit(NearToken::from_near(1));
        testing_env!(ctx.build());
        let _ = c.create_wallet(
            "sentinel-task".to_string(),
            pk(),
            3600,
            U128::from(1_000_000_000_000_000_000_000_000u128),
            Some(2),
            Some(60),
        );
    }

    fn wallet_ctx(task_wallet: &AccountId, ts: u64) {
        let mut ctx = context();
        ctx.signer_account_id(task_wallet.clone());
        ctx.predecessor_account_id(task_wallet.clone());
        ctx.block_timestamp(ts);
        ctx.prepaid_gas(Gas::from_tgas(100));
        testing_env!(ctx.build());
    }

    #[test]
    #[should_panic(expected = "wallet quarantined")]
    fn quarantined_wallet_refuses_all_calls() {
        let ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        sentinel_wallet(&mut c);
        let wallet = c.get_task("sentinel-task".to_string()).unwrap().wallet;
        // 3 rapid calls within the window: 3rd attempt quarantines... actually
        // the 3rd SILENTLY quarantines (no panic), so simulate: two ok calls, third locks
        for i in 0..2u64 {
            wallet_ctx(&wallet, i);
            let _ = c.agent_call(
                "sentinel-task".to_string(),
                "api-provider.near".parse().unwrap(),
                U128::from(100_000_000_000_000_000_000_000u128),
                format!("call-{i}"),
            );
        }
        wallet_ctx(&wallet, 2);
        let _ = c.agent_call(
            "sentinel-task".to_string(),
            "api-provider.near".parse().unwrap(),
            U128::from(100_000_000_000_000_000_000_000u128),
            "burst".to_string(),
        );
        assert!(c.get_task("sentinel-task".to_string()).unwrap().quarantined);
        // any further call panics on quarantine gate
        wallet_ctx(&wallet, 3);
        let _ = c.agent_call(
            "sentinel-task".to_string(),
            "api-provider.near".parse().unwrap(),
            U128::from(1),
            "after-lock".to_string(),
        );
    }

    #[test]
    fn velocity_window_resets_over_time() {
        let ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        sentinel_wallet(&mut c);
        let wallet = c.get_task("sentinel-task".to_string()).unwrap().wallet;
        for i in 0..2u64 {
            wallet_ctx(&wallet, i);
            let _ = c.agent_call(
                "sentinel-task".to_string(),
                "api-provider.near".parse().unwrap(),
                U128::from(100_000_000_000_000_000_000_000u128),
                format!("call-{i}"),
            );
        }
        // far outside the 60s window -> old calls expire, allowed again
        wallet_ctx(&wallet, 120 * 1_000_000_000);
        let _ = c.agent_call(
            "sentinel-task".to_string(),
            "api-provider.near".parse().unwrap(),
            U128::from(100_000_000_000_000_000_000_000u128),
            "later".to_string(),
        );
        assert!(!c.get_task("sentinel-task".to_string()).unwrap().quarantined);
    }

    #[test]
    fn canary_payment_sweeps_and_locks() {
        let ctx = context();
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        sentinel_wallet(&mut c);
        c.set_canary("bait.near".parse().unwrap());
        let wallet = c.get_task("sentinel-task".to_string()).unwrap().wallet;
        wallet_ctx(&wallet, 0);
        let _ = c.agent_call(
            "sentinel-task".to_string(),
            "bait.near".parse().unwrap(),
            U128::from(100_000_000_000_000_000_000_000u128),
            "oops".to_string(),
        );
        let t = c.get_task("sentinel-task".to_string()).unwrap();
        assert!(t.quarantined);
        assert!(t.completed);
    }

    #[test]
    #[should_panic(expected = "max_calls_per_window must be")]
    fn insane_velocity_cap_rejected() {
        let mut ctx = context();
        ctx.attached_deposit(NearToken::from_near(1));
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        let _ = c.create_wallet(
            "gas-grief".to_string(),
            pk(),
            3600,
            U128::from(1_000_000_000_000_000_000_000_000u128),
            Some(50_000),
            Some(60),
        );
    }

    #[test]
    #[should_panic(expected = "velocity window must be")]
    fn insane_velocity_window_rejected() {
        let mut ctx = context();
        ctx.attached_deposit(NearToken::from_near(1));
        testing_env!(ctx.build());
        let mut c = Contract::new(accounts(2));
        let _ = c.create_wallet(
            "big-window".to_string(),
            pk(),
            3600,
            U128::from(1_000_000_000_000_000_000_000_000u128),
            Some(30),
            Some(400 * 24 * 3600),
        );
    }
}

#[cfg(test)]
mod fuzz {
    use super::*;
    use near_sdk::test_utils::{VMContextBuilder, accounts};
    use near_sdk::{Gas, testing_env};

    fn ctx(deposit: u128) -> VMContextBuilder {
        let mut b = VMContextBuilder::new();
        b.current_account_id(accounts(1));
        b.signer_account_id(accounts(2));
        b.predecessor_account_id(accounts(2));
        b.attached_deposit(NearToken::from_yoctonear(deposit));
        b.prepaid_gas(Gas::from_tgas(300));
        b
    }

    // Hostile task_id corpus: traversal, unicode, dots, empty, long, edge chars
    #[test]
    fn fuzz_hostile_task_ids() {
        let corpus = [
            "",
            "a",
            ".",
            "..",
            "a.b",
            "a..b",
            ".a",
            "a.",
            "-a",
            "a-",
            "A",
            "aB",
            "a b",
            "a/b",
            "a\\b",
            "a\u{0000}b",
            "\u{2603}",
            "0",
            "9",
            "aaa",
            &"x".repeat(33),
            &"y".repeat(32),
            &"z".repeat(64),
            "con",
            "nul",
            "a-b_c9",
            "--",
            "__",
            "-_-",
            "0-9_az",
            "\u{00e9}tu",
            "task id",
            "task;drop",
            "task%00",
            "\u{200d}task",
        ];
        for id in corpus {
            let mut c = VMContextBuilder::new();
            c.current_account_id(accounts(1));
            c.signer_account_id(accounts(2));
            c.predecessor_account_id(accounts(2));
            c.attached_deposit(NearToken::from_near(1));
            testing_env!(c.build());
            let mut contract = Contract::new(accounts(2));
            // Must either succeed cleanly or panic cleanly - NEVER corrupt state
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = contract.create_wallet(
                    id.to_string(),
                    "ed25519:8h7kFK4t1Pj5wMsmvB5NmMBiF9YZTfbvJdeWPDXCCbT8"
                        .parse()
                        .unwrap(),
                    3600,
                    U128::from(1_000_000_000_000_000_000_000_000u128),
                    Some(10),
                    Some(60),
                );
            }));
            // State must remain consistent after ANY outcome
            let mut c2 = VMContextBuilder::new();
            c2.current_account_id(accounts(1));
            c2.signer_account_id(accounts(2));
            c2.predecessor_account_id(accounts(2));
            testing_env!(c2.build());
            let _ = contract.get_owner(); // reading state never panics
            let _ = result;
        }
    }

    // Hostile agent_call amounts: zero, max u128, overflow pairs, odd values
    #[test]
    fn fuzz_hostile_amounts() {
        let c = ctx(1_000_000_000_000_000_000_000_000);
        testing_env!(c.build());
        let mut contract = Contract::new(accounts(2));
        let _ = contract.create_wallet(
            "fuzz-amt".to_string(),
            "ed25519:8h7kFK4t1Pj5wMsmvB5NmMBiF9YZTfbvJdeWPDXCCbT8"
                .parse()
                .unwrap(),
            3600,
            U128::from(1_000_000_000_000_000_000_000_000u128),
            Some(10),
            Some(60),
        );
        let wallet = contract.get_task("fuzz-amt".to_string()).unwrap().wallet;
        let amounts = [
            0u128,
            1,
            999_999_999_999_999_999_999_999,
            1_000_000_000_000_000_000_000_000,
            1_000_000_000_000_000_000_000_001,
            u128::MAX,
            u128::MAX - 1,
            u128::MAX / 2,
            50_000_000_000_000_000_000_000, // exact allowance boundary
        ];
        for amt in amounts {
            let mut c = VMContextBuilder::new();
            c.current_account_id(accounts(1));
            c.signer_account_id(wallet.clone());
            c.predecessor_account_id(wallet.clone());
            c.prepaid_gas(Gas::from_tgas(100));
            testing_env!(c.build());
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = contract.agent_call(
                    "fuzz-amt".to_string(),
                    "api.near".parse().unwrap(),
                    U128::from(amt),
                    "fuzz".to_string(),
                );
            }));
            // spent must never exceed cap
            let t = contract.get_task("fuzz-amt".to_string()).unwrap();
            assert!(t.spent <= t.spend_cap, "CAP VIOLATED at {}", amt);
        }
    }
}
