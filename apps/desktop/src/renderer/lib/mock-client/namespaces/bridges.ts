import type { EmailWatchConnectRequest, SyncConfigureRequest, SyncStatus } from '@ajh/shared';

import type { AppClient } from '../../app-client';
import { unsub } from './helpers';

export const bridgeNamespaces = (): Pick<AppClient, 'extensionBridge' | 'emailWatch' | 'sync'> => {
  let syncStatus: SyncStatus = { enabled: false, insecure: false };
  return {
    extensionBridge: {
      status: async () => ({
        port: 47615,
        connected: false,
        lastSeenMs: null,
        token: 'mock-token',
      }),
      regenerateToken: async () => ({ token: 'mock-token' }),
      autofillEnabled: async () => ({ enabled: false }),
      setAutofillEnabled: async (enabled: boolean) => ({ enabled }),
      aiAssistEnabled: async () => ({ enabled: false }),
      setAiAssistEnabled: async (enabled: boolean) => ({ enabled }),
      autoTrackEnabled: async () => ({ enabled: false }),
      setAutoTrackEnabled: async (enabled: boolean) => ({ enabled }),
      onChanged: unsub,
    },

    // autoWriteEnabled defaults to false — matches the real backend default
    // (opt-in only, after five security rounds on the sender-authentication
    // gate's known-imperfect check).
    emailWatch: {
      status: async () => ({ connected: false, enabled: false, autoWriteEnabled: false }),
      connect: async ({ address }: EmailWatchConnectRequest) => ({
        connected: true,
        address,
        enabled: false,
        autoWriteEnabled: false,
      }),
      disconnect: async () => ({ connected: false, enabled: false, autoWriteEnabled: false }),
      setEnabled: async (enabled: boolean) => ({
        connected: false,
        enabled,
        autoWriteEnabled: false,
      }),
      setAutoWriteEnabled: async (autoWriteEnabled: boolean) => ({
        connected: false,
        enabled: false,
        autoWriteEnabled,
      }),
      checkNow: async () => ({ connected: false, enabled: false, autoWriteEnabled: false }),
    },
    // Optional self-hosted sync: off in the mock until `configure`, then kept
    // in this closure so `status`/`syncNow` agree with it. No network.
    sync: {
      status: async () => syncStatus,
      configure: async ({ serverUrl }: SyncConfigureRequest) => {
        syncStatus = { enabled: true, serverUrl, insecure: serverUrl.startsWith('http://') };
        return syncStatus;
      },
      disable: async () => {
        syncStatus = { enabled: false, insecure: false };
        return syncStatus;
      },
      syncNow: async () => syncStatus,
    },
  };
};
