import { invoke } from '@tauri-apps/api/core';

import type { SyncConfigureRequest, SyncStatus } from '@ajh/shared';

export const sync = {
  status: () => invoke<SyncStatus>('sync_status'),
  configure: (req: SyncConfigureRequest) =>
    invoke<SyncStatus>('sync_configure', { serverUrl: req.serverUrl, token: req.token }),
  disable: () => invoke<SyncStatus>('sync_disable'),
  syncNow: () => invoke<SyncStatus>('sync_now'),
};
