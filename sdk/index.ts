/**
 * FUKILLA SDK — "Burn after paying."
 * The 5-line integration for on-chain self-quarantining agent wallets.
 *
 * Zero dependencies beyond near-api-js peer. Works in Node 18+.
 *
 * QUICKSTART:
 *   import { Fukilla } from "fukilla";
 *   const fuk = new Fukilla({ factory: "fukilla.testnet", network: "testnet" });
 *   const task = await fuk.createWallet({ fundingNEAR: 1, capNEAR: 0.05, ttlSec: 3600 });
 *   // hand task.agentSecret ONLY to your agent; it can pay but never own
 *   await fuk.completeTask(task.taskId); // sweep-back + wallet destroyed
 */
import { KeyPair, connect } from "near-api-js";

export interface FukillaConfig {
  /** Factory contract account (e.g. "fukilla.testnet") */
  factory: string;
  /** "testnet" | "mainnet" */
  network?: string;
  /** near-api-js connection (advanced) */
  connection?: any;
}

export interface CreateWalletOpts {
  /** Total deposit attached to fund the ephemeral wallet */
  fundingNEAR: number;
  /** Cumulative spend cap for the agent (yoctoNEAR as string or NEAR number) */
  capNEAR: number | string;
  /** Wallet lifetime in seconds (max 30 days) */
  ttlSec: number;
  /** SENTINEL: max payments inside the velocity window (optional) */
  maxCallsPerWindow?: number;
  /** SENTINEL: velocity window in seconds (optional) */
  velocityWindowSec?: number;
  /** Unique task id; auto-generated when omitted */
  taskId?: string;
}

export interface TaskHandle {
  taskId: string;
  wallet: string;
  /** Full-access secret for the WALLET owner (keep human-side!) */
  ownerSecret: string;
  /** Function-call keypair the AGENT receives (restricted to agent_call) */
  agentSecret: string;
  agentPublicKey: string;
}

const N = (nea: number | string): string =>
  typeof nea === "string"
    ? nea
    : BigInt(Math.round(nea * 1e24)).toString();

export class Fukilla {
  private cfg: Required<Pick<FukillaConfig, "factory">> & FukillaConfig;

  constructor(cfg: FukillaConfig) {
    if (!cfg.factory) throw new Error("factory account required");
    this.cfg = cfg;
  }

  /** Create an ephemeral wallet. Owner keypair + agent FC keypair generated locally. */
  async createWallet(opts: CreateWalletOpts): Promise<TaskHandle> {
    const near = await this.near();
    const ownerKp = KeyPair.fromRandom("ed25519");
    const agentKp = KeyPair.fromRandom("ed25519");
    const taskId =
      opts.taskId ?? `task-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`;

    // Fund a throwaway owner account via the factory's account as signer.
    // For production, the caller supplies a funded owner account connection.
    const account = await near.account(this.cfg.factory);

    await account.functionCall({
      contractId: this.cfg.factory,
      methodName: "create_wallet",
      args: {
        task_id: taskId,
        agent_key: agentKp.getPublicKey().toString(),
        ttl_sec: opts.ttlSec,
        spend_cap: N(opts.capNEAR),
        max_calls_per_window: opts.maxCallsPerWindow ?? null,
        velocity_window_sec: opts.velocityWindowSec ?? null,
      },
      attachedDeposit: BigInt(N(opts.fundingNEAR)),
      gas: BigInt("300000000000000"),
    });

    return {
      taskId,
      wallet: `${taskId}.${this.cfg.factory}`,
      ownerSecret: ownerKp.toString(),
      agentSecret: agentKp.toString(),
      agentPublicKey: agentKp.getPublicKey().toString(),
    };
  }

  /** Agent pays an API provider through the factory's only agent-facing door. */
  static agentCallPayload(taskId: string, receiverId: string, amountNEAR: number | string, memo: string) {
    return {
      contractId_method: "agent_call",
      args: { task_id: taskId, receiver_id: receiverId, amount: N(amountNEAR), memo },
    };
  }

  /** Owner closes the task: remaining balance sweeps home, wallet self-destructs. */
  async completeTask(taskId: string, ownerAccountId: string) {
    const near = await this.near();
    const account = await near.account(ownerAccountId);
    return account.functionCall({
      contractId: this.cfg.factory,
      methodName: "complete_task",
      args: { task_id: taskId },
      gas: BigInt("300000000000000"),
    });
  }

  /** Read-only: task status incl. SENTINEL quarantine flag. */
  async getTask(taskId: string): Promise<any> {
    const near = await this.near();
    return near.connection.provider.query({
      request_type: "call_function",
      account_id: this.cfg.factory,
      method_name: "get_task",
      args_base64: Buffer.from(JSON.stringify({ task_id: taskId })).toString("base64"),
      finality: "optimistic",
    }).then((r: any) => JSON.parse(Buffer.from(r.result).toString()));
  }

  private async near() {
    if (this.cfg.connection) return this.cfg.connection;
    const { connect } = await import("near-api-js");
    const network = this.cfg.network ?? "testnet";
    return connect({
      networkId: network,
      nodeUrl: `https://rpc.${network}.near.org`,
    });
  }
}

export default Fukilla;
