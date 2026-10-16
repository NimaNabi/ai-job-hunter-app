import { describe, expect, it, vi } from 'vitest';
import { act, waitFor } from '@testing-library/react';

import type { SyncStatus } from '@ajh/shared';

import { createMockClient, exerciseServiceHooks, renderHookWithClient } from '@/test-support';

import { keys } from '../query-client';
import * as mod from './use-sync';
import { useConfigureSync, useDisableSync, useSyncNow, useSyncStatus } from './use-sync';

const OFF: SyncStatus = { enabled: false, insecure: false };
const ON: SyncStatus = {
  enabled: true,
  serverUrl: 'https://sync.example.org',
  insecure: false,
  lastSyncAt: 1_700_000_000_000,
  lastReport: { pulled: 3, applied: 2, held: 0, pushed: 1 },
};

describe('use-sync services', () => {
  it('renders every exported hook without crashing', async () => {
    await exerciseServiceHooks(mod);
  });
});

describe('useSyncStatus', () => {
  it('returns the status payload from the client', async () => {
    const client = createMockClient({ 'sync.status': vi.fn().mockResolvedValue(ON) });
    const { result } = renderHookWithClient(() => useSyncStatus(), { client });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(ON);
  });
});

describe('useConfigureSync', () => {
  it('forwards url + token and seeds the status cache', async () => {
    const configure = vi.fn().mockResolvedValue(ON);
    const client = createMockClient({
      'sync.status': vi.fn().mockResolvedValue(OFF),
      'sync.configure': configure,
    });
    const { result, queryClient } = renderHookWithClient(() => useConfigureSync(), { client });
    await act(async () => {
      await result.current.mutateAsync({ serverUrl: 'https://sync.example.org', token: 't' });
    });
    expect(configure).toHaveBeenCalledWith({ serverUrl: 'https://sync.example.org', token: 't' });
    await waitFor(() => expect(queryClient.getQueryData(keys.sync.status)).toEqual(ON));
  });
});

describe('useDisableSync and useSyncNow', () => {
  it('seed the status cache with what the backend echoes', async () => {
    const client = createMockClient({
      'sync.status': vi.fn().mockResolvedValue(ON),
      'sync.disable': vi.fn().mockResolvedValue(OFF),
      'sync.syncNow': vi.fn().mockResolvedValue(ON),
    });
    const disable = renderHookWithClient(() => useDisableSync(), { client });
    await act(async () => {
      await disable.result.current.mutateAsync();
    });
    await waitFor(() => expect(disable.queryClient.getQueryData(keys.sync.status)).toEqual(OFF));

    const now = renderHookWithClient(() => useSyncNow(), { client });
    await act(async () => {
      await now.result.current.mutateAsync();
    });
    await waitFor(() => expect(now.queryClient.getQueryData(keys.sync.status)).toEqual(ON));
  });
});
