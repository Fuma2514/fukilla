//! Minimal self-destruct executor deployed on every ephemeral wallet
//! sub-account. The ONLY method deletes the wallet account and sweeps the
//! entire remaining balance to the beneficiary (the task owner).
//!
//! Guard: only the factory contract (the wallet's parent) may call this.
//! The agent's function-call key cannot reach this method — its key is
//! restricted to `agent_call` on the factory.
use near_sdk::{AccountId, NearToken, env, near, require};

#[near(contract_state)]
#[derive(Default)]
pub struct Executor;

#[near]
impl Executor {
    /// Delete this wallet account, sending every remaining yoctoNEAR to
    /// `beneficiary`. Callable only by the factory contract.
    pub fn sweep(&self, beneficiary: AccountId) {
        require!(
            env::predecessor_account_id()
                == env::current_account_id()
                    .as_str()
                    .split_once('.')
                    .map(|(_, parent)| parent.to_string())
                    .unwrap_or_default(),
            "only the factory can sweep this wallet"
        );
        env::log_str(&format!(
            "{{\"event\":\"wallet_sweep\",\"beneficiary\":\"{}\"}}",
            beneficiary
        ));
        near_sdk::Promise::new(env::current_account_id()).delete_account(beneficiary);
    }

    /// Pay `amount` yoctoNEAR from THIS wallet's own balance to `receiver`.
    /// Callable only by the factory (the wallet's parent), which is where all
    /// FUKILLA policy checks (spend cap, velocity, canary, expiry, quarantine)
    /// have already passed. Funds genuinely leave the sub-account — the task's
    /// own balance — never the factory's.
    pub fn pay(receiver: AccountId, amount: near_sdk::json_types::U128) {
        require!(
            env::predecessor_account_id()
                == env::current_account_id()
                    .as_str()
                    .split_once('.')
                    .map(|(_, parent)| parent.to_string())
                    .unwrap_or_default(),
            "only the factory can pay from this wallet"
        );
        let amount: u128 = amount.into();
        // NOTE: no explicit reserve check — a transfer larger than the
        // wallet's balance simply fails at the runtime level, and the
        // factory's spend_cap already bounds the total outflow.
        near_sdk::Promise::new(receiver).transfer(NearToken::from_yoctonear(amount));
    }
}
