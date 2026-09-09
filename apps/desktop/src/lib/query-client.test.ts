import { QueryClient, onlineManager } from '@tanstack/react-query';
import { afterEach, describe, expect, it } from 'vitest';
import { createQueryClient } from './query-client';

/**
 * The offline guarantee at the data layer (ADR-041).
 *
 * With the browser reporting no network, a save and a read must still run to
 * completion: neither ever touches the network, so connectivity is not
 * theirs to wait for. The last test proves the library really would pause
 * them otherwise — a green run above it means something.
 */
describe('createQueryClient', () => {
  afterEach(() => {
    onlineManager.setOnline(true);
  });

  it('declares that neither queries nor mutations depend on connectivity', () => {
    const defaults = createQueryClient().getDefaultOptions();
    expect(defaults.queries?.networkMode).toBe('always');
    expect(defaults.mutations?.networkMode).toBe('always');
  });

  it('runs a mutation while the browser reports offline', async () => {
    onlineManager.setOnline(false);
    const client = createQueryClient();
    const mutation = client.getMutationCache().build(client, {
      mutationFn: async (name: string) => `נשמר: ${name}`,
    });

    await expect(mutation.execute('שרה')).resolves.toBe('נשמר: שרה');
    expect(mutation.state.isPaused).toBe(false);
  });

  it('runs a query while the browser reports offline', async () => {
    onlineManager.setOnline(false);
    const client = createQueryClient();

    await expect(
      client.fetchQuery({ queryKey: ['contact', '1'], queryFn: async () => ({ id: '1' }) }),
    ).resolves.toEqual({ id: '1' });
  });

  it('would have paused the save with the library defaults', async () => {
    onlineManager.setOnline(false);
    const client = new QueryClient();
    const mutation = client.getMutationCache().build(client, {
      mutationFn: async () => 'never',
    });
    // Left pending on purpose: nothing resumes a paused mutation here, and
    // that is the point.
    void mutation.execute(undefined).catch(() => undefined);

    const outcome = await Promise.race([
      new Promise<string>((resolve) => setTimeout(() => resolve('still waiting'), 50)),
    ]);
    expect(outcome).toBe('still waiting');
    expect(mutation.state.isPaused).toBe(true);
  });
});
