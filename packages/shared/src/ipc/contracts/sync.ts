/**
 * Optional self-hosted sync (ADR-051, protocol in `docs/knowledge/sync-protocol.md`).
 *
 * Off by default. The app is a client only: the user runs their own server and gives
 * the app its URL and a token. `configure` checks the server's `/health`, stores the
 * token in the OS keychain, turns sync on and runs once. The backend then syncs
 * shortly after launch, every 15 minutes, and on `syncNow`. The token is write-only:
 * sent once to `configure`, never returned or logged.
 */

/** Counts from one sync run. */
export interface SyncRunReport {
  /** Changes downloaded from the server. */
  pulled: number;
  /** Downloaded changes written to this device. */
  applied: number;
  /** Changes from a newer app version, kept until this app is updated. */
  held: number;
  /** Local changes uploaded. */
  pushed: number;
}

export interface SyncStatus {
  enabled: boolean;
  serverUrl?: string;
  /** The server URL is `http://`: traffic is unencrypted unless the link itself
   *  is private (a trusted LAN, Tailscale). The UI must warn. */
  insecure: boolean;
  /** Epoch ms of the last successful run. */
  lastSyncAt?: number;
  /** The last run's error, cleared by a successful run. */
  lastError?: string;
  lastReport?: SyncRunReport;
}

export interface SyncConfigureRequest {
  serverUrl: string;
  token: string;
}

export interface SyncContract {
  status(): Promise<SyncStatus>;
  /** Checks the server, saves the URL and the token (OS keychain), turns sync on and
   *  runs once. Rejects, saving nothing, when the server is unreachable, speaks
   *  another protocol version, or rejects the token. */
  configure(req: SyncConfigureRequest): Promise<SyncStatus>;
  /** Turns sync off and removes the token. Local data is untouched. */
  disable(): Promise<SyncStatus>;
  /** Runs one sync now. Rejects when sync is off or a run is in progress. */
  syncNow(): Promise<SyncStatus>;
}

export const SYNC_CHANNELS = {
  status: 'sync:status',
  configure: 'sync:configure',
  disable: 'sync:disable',
  syncNow: 'sync:syncNow',
} as const;
