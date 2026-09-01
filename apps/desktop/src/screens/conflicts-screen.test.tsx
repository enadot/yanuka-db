// @vitest-environment jsdom
import '@testing-library/jest-dom/vitest';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { MemoryRouter } from 'react-router-dom';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { OpenConflict } from '../lib/desktop-io';

const openConflicts = vi.fn();
const resolveConflict = vi.fn();

vi.mock('../lib/desktop-io', () => ({
  openConflicts: () => openConflicts(),
  resolveConflict: (id: string, choices: unknown) => resolveConflict(id, choices),
}));

const { ConflictsScreen } = await import('./conflicts-screen');

/** Two devices that disagree about a city and a profession. */
const conflict: OpenConflict = {
  id: 'conflict-1',
  entityType: 'contact',
  entityId: 'contact-1',
  displayName: 'יעקב פרידמן',
  detectedAt: '2026-08-20T10:00:00Z',
  fields: [
    {
      field: 'city',
      localValue: 'אנטוורפן',
      remoteValue: 'לונדון',
      localUpdatedAt: '2026-08-19T09:00:00Z',
      remoteUpdatedAt: '2026-08-20T08:00:00Z',
      localDeviceId: 'device-a',
      remoteDeviceId: 'device-b',
    },
    {
      field: 'profession',
      localValue: 'סופר סת"ם',
      remoteValue: 'מגיה',
      localUpdatedAt: '2026-08-19T09:00:00Z',
      remoteUpdatedAt: '2026-08-20T08:00:00Z',
      localDeviceId: 'device-a',
      remoteDeviceId: 'device-b',
    },
  ],
};

function draw() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={client}>
      <MemoryRouter>
        <ConflictsScreen />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe('ConflictsScreen', () => {
  // Without this, every test after the first renders into a document that still
  // holds the previous one, and an unambiguous `getByText` starts failing for
  // reasons unrelated to the code under test.
  afterEach(cleanup);

  beforeEach(() => {
    openConflicts.mockReset().mockResolvedValue([conflict]);
    resolveConflict.mockReset().mockResolvedValue(undefined);
  });

  it('shows both answers to every question, in words rather than field names', async () => {
    draw();

    expect(await screen.findByText('יעקב פרידמן')).toBeInTheDocument();
    expect(screen.getByText('עיר')).toBeInTheDocument();
    expect(screen.getByText('מקצוע')).toBeInTheDocument();
    expect(screen.getByText('אנטוורפן')).toBeInTheDocument();
    expect(screen.getByText('לונדון')).toBeInTheDocument();
    // The raw field name never reaches the screen.
    expect(screen.queryByText('city')).not.toBeInTheDocument();
  });

  it('preselects nothing, so no decision is made on the user’s behalf', async () => {
    draw();
    await screen.findByText('יעקב פרידמן');

    expect(screen.queryAllByRole('button', { pressed: true })).toHaveLength(0);
    expect(screen.getAllByRole('button', { pressed: false })).toHaveLength(4);
    expect(screen.getByTestId('save-conflict-choice')).toBeDisabled();
  });

  it('sends only the fields that were actually decided', async () => {
    const user = userEvent.setup();
    draw();
    await screen.findByText('יעקב פרידמן');

    await user.click(screen.getByText('לונדון'));
    expect(screen.getByText(/נבחרו 1 מתוך 2/)).toBeInTheDocument();

    await user.click(screen.getByTestId('save-conflict-choice'));

    await waitFor(() => expect(resolveConflict).toHaveBeenCalledTimes(1));
    expect(resolveConflict).toHaveBeenCalledWith('conflict-1', [
      { field: 'city', side: 'remote' },
    ]);
  });

  it('replaces a choice rather than adding to it when the other side is clicked', async () => {
    const user = userEvent.setup();
    draw();
    await screen.findByText('יעקב פרידמן');

    await user.click(screen.getByText('לונדון'));
    await user.click(screen.getByText('אנטוורפן'));
    await user.click(screen.getByTestId('save-conflict-choice'));

    await waitFor(() => expect(resolveConflict).toHaveBeenCalledTimes(1));
    expect(resolveConflict).toHaveBeenCalledWith('conflict-1', [
      { field: 'city', side: 'local' },
    ]);
  });

  it('says there is nothing to decide when there is nothing to decide', async () => {
    openConflicts.mockResolvedValue([]);
    draw();

    expect(await screen.findByText('אין מה להכריע')).toBeInTheDocument();
  });

  it('marks a value that was cleared on one side rather than showing a blank', async () => {
    openConflicts.mockResolvedValue([
      { ...conflict, fields: [{ ...conflict.fields[0], remoteValue: null }] },
    ]);
    draw();

    expect(await screen.findByText('ריק')).toBeInTheDocument();
  });
});
