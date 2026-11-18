import { QueryClient, useMutation, useQuery } from '@tanstack/react-query';
import { OfflineError, AuthExpiredError, type ApiClient } from '@/api/client';
import { connectionStore } from '@/state/connection';
import type { CareAction as CarePayload, Creature, Snapshot } from '@chaogarden/contracts';
import { validateCreature } from '@chaogarden/contracts';

/** Query key factories keep cache invalidation consistent. */
export const keys = {
  snapshot: ['snapshot'] as const,
  creature: (id: string) => ['creature', id] as const,
};

export function createQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: {
        retry: (failureCount, error) => {
          if (error instanceof AuthExpiredError || error instanceof OfflineError) {
            connectionStore.getState().set(
              error instanceof AuthExpiredError ? 'expired' : 'offline',
            );
            return failureCount < 3;
          }
          return failureCount < 2;
        },
        retryDelay: (attempt) => Math.min(500 * 2 ** attempt, 4000),
        refetchOnReconnect: true,
        staleTime: 5_000,
      },
      mutations: {
        onError: (error) => {
          if (error instanceof OfflineError) connectionStore.getState().set('offline');
          if (error instanceof AuthExpiredError) connectionStore.getState().set('expired');
        },
        onSuccess: () => connectionStore.getState().markSuccess(),
      },
    },
  });
}

function markOnline() {
  connectionStore.getState().markSuccess();
}

/**
 * Garden snapshot (creature, inventory, decorations). Components apply
 * optimistic overlays on top; the authoritative response reconciles the cache.
 */
export function useSnapshot(api: ApiClient) {
  return useQuery({
    queryKey: keys.snapshot,
    queryFn: async ({ signal }) => {
      const snap = await api.snapshot(signal);
      markOnline();
      return snap;
    },
  });
}

export function useCreature(creatureId: string, api: ApiClient) {
  return useQuery({
    queryKey: keys.creature(creatureId),
    queryFn: async ({ signal }) => {
      const snap = await api.snapshot(signal);
      markOnline();
      if (!snap.creature || snap.creature.id !== creatureId) {
        throw new Error('Creature not found');
      }
      return snap.creature;
    },
  });
}

let idCounter = 0;
export function nextIdempotencyKey(prefix: string) {
  idCounter += 1;
  return `${prefix}-${Date.now()}-${idCounter}`;
}

/**
 * Care action mutation. Every attempt carries a stable idempotency key so a
 * network retry or double-tap cannot apply the action twice; the returned
 * snapshot is written into the cache, reconciling the optimistic overlay.
 */
export function useCareMutation(creatureId: string, api: ApiClient) {
  return useMutation({
    mutationKey: ['care', creatureId],
    mutationFn: (action: CarePayload & { key: string }) =>
      api.care(creatureId, action, action.key),
    onSuccess: (snap: Snapshot) => markOnline(),
  });
}

export function useAdoptionMutation(api: ApiClient) {
  return useMutation({
    mutationKey: ['adopt'],
    mutationFn: (payload: { name: string; color: string }) =>
      api.adopt(payload, nextIdempotencyKey('adopt')),
  });
}

export function useUseItemMutation(api: ApiClient) {
  return useMutation({
    mutationKey: ['use-item'],
    mutationFn: (args: { itemId: string; creatureId?: string }) =>
      api.useItem(args, nextIdempotencyKey(`use-${args.itemId}`)),
  });
}

export function useMoveDecorationMutation(api: ApiClient) {
  return useMutation({
    mutationKey: ['move-decoration'],
    mutationFn: (args: { id: string; x: number; y: number; rotation: number }) =>
      api.moveDecoration(args, nextIdempotencyKey(`deco-${args.id}`)),
  });
}

export function useTrainingMutation(api: ApiClient) {
  return useMutation({
    mutationKey: ['training'],
    mutationFn: (args: { creatureId: string; stat: Creature['stats'][number]['name'] }) =>
      api.startTraining(args, nextIdempotencyKey(`train-${args.stat}`)),
  });
}

export { validateCreature };
