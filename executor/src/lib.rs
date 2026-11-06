//! Minimal self-destruct executor deployed on every ephemeral wallet
//! sub-account. The ONLY method deletes the wallet account and sweeps the
//! entire remaining balance to the beneficiary (the task owner).
//!
//! Guard: only the factory contract (the wallet's parent) may call this.
//! The agent's function-call key cannot reach this method — its key is
//! restricted to `agent_call` on the factory.
use near_sdk::{AccountId, env, near, require};

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
}
