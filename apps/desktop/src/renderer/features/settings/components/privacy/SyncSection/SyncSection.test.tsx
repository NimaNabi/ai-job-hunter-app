import type { ReactNode } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { QueryClientProvider } from '@tanstack/react-query';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';

import type { SyncStatus } from '@ajh/shared';
import { NotificationProvider } from '@ajh/ui';

import { AppClientProvider } from '@/providers/AppClientProvider';
import { createMockClient, makeQueryClient } from '@/test-support';

import { SyncSection } from './index';

const OFF: SyncStatus = { enabled: false, insecure: false };
const ON: SyncStatus = {
  enabled: true,
  serverUrl: 'http://100.64.0.1:8787',
  insecure: true,
  lastSyncAt: 1_700_000_000_000,
  lastReport: { pulled: 7, applied: 7, held: 0, pushed: 0 },
};

function renderSection(overrides: Record<string, (...args: never[]) => unknown> = {}) {
  const client = createMockClient({
    'sync.status': vi.fn().mockResolvedValue(OFF),
    ...overrides,
  });
  const queryClient = makeQueryClient();

  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <AppClientProvider client={client}>
          <NotificationProvider>{children}</NotificationProvider>
        </AppClientProvider>
      </QueryClientProvider>
    );
  }

  return render(<SyncSection />, { wrapper: Wrapper });
}

describe('SyncSection — off → turn on', () => {
  it('sends the URL and token, then hides the token field', async () => {
    const configure = vi.fn().mockResolvedValue(ON);
    renderSection({ 'sync.configure': configure });

    await waitFor(() => screen.getByLabelText('Server URL'));
    await userEvent.type(screen.getByLabelText('Server URL'), 'http://100.64.0.1:8787');
    await userEvent.type(screen.getByLabelText('Sync token'), 'secret-token');
    expect(screen.getByText(/traffic is not encrypted/)).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Turn on sync' }));

    await waitFor(() =>
      expect(configure).toHaveBeenCalledWith({
        serverUrl: 'http://100.64.0.1:8787',
        token: 'secret-token',
      })
    );
    await waitFor(() => expect(screen.queryByLabelText('Sync token')).not.toBeInTheDocument());
    expect(screen.getByText('Last run: 7 received, 7 applied, 0 sent')).toBeInTheDocument();
  });

  it('shows a fixed message when turning on fails', async () => {
    renderSection({ 'sync.configure': vi.fn().mockRejectedValue(new Error('raw detail')) });
    await waitFor(() => screen.getByLabelText('Server URL'));
    await userEvent.type(screen.getByLabelText('Server URL'), 'https://sync.example.org');
    await userEvent.type(screen.getByLabelText('Sync token'), 't');
    await userEvent.click(screen.getByRole('button', { name: 'Turn on sync' }));
    await waitFor(() => expect(screen.getByText(/Couldn't turn on sync/)).toBeInTheDocument());
    expect(screen.queryByText('raw detail')).not.toBeInTheDocument();
  });
});

describe('SyncSection — on', () => {
  it('runs a sync and can turn sync off', async () => {
    const syncNow = vi.fn().mockResolvedValue(ON);
    const disable = vi.fn().mockResolvedValue(OFF);
    renderSection({
      'sync.status': vi.fn().mockResolvedValue({ ...ON, lastError: 'sync server unreachable' }),
      'sync.syncNow': syncNow,
      'sync.disable': disable,
    });

    await waitFor(() => screen.getByRole('button', { name: 'Sync now' }));
    expect(screen.getByText('Last attempt failed: sync server unreachable')).toBeInTheDocument();

    await userEvent.click(screen.getByRole('button', { name: 'Sync now' }));
    await waitFor(() => expect(syncNow).toHaveBeenCalled());

    await userEvent.click(screen.getByRole('button', { name: 'Turn off sync' }));
    await waitFor(() => expect(disable).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByLabelText('Server URL')).toBeInTheDocument());
  });
});
