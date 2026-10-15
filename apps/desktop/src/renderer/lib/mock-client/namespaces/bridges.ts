import type { EmailWatchConnectRequest, SyncConfigureRequest } from '@ajh/shared';

import type { AppClient } from '../../app-client';
import { unsub } from './helpers';

export const bridgeNamespaces = (): Pick<AppClient, 'extensionBridge' | 'emailWatch' | 'sync'> => ({
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
  // Optional self-hosted sync: off in the mock; `configure` echoes an enabled,
  // never-synced status without any network.
  sync: {
    status: async () => ({ enabled: false, insecure: false }),
    configure: async ({ serverUrl }: SyncConfigureRequest) => ({
      enabled: true,
      serverUrl,
      insecure: serverUrl.startsWith('http://'),
    }),
    disable: async () => ({ enabled: false, insecure: false }),
    syncNow: async () => ({ enabled: false, insecure: false }),
  },
});
