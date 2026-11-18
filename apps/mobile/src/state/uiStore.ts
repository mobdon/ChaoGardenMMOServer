import { create } from 'zustand';

/**
 * Zustand is used ONLY for local UI state (which is not owned by the server):
 * the decoration editor draft, pending optimistic care/seed overlays, the
 * adoption flow's local draft, and settings like reduced-motion preference.
 * Creature data itself is React Query server state.
 */

export type DecorationDraft = {
  id: string;
  catalogId: string;
  x: number;
  y: number;
  rotation: number;
};

export type AdoptionDraft = {
  name: string;
  color: string;
  mode: 'guest' | 'account' | null;
};

export type SettingsState = {
  reduceMotionOverride: boolean | null;
};

type UiState = {
  adoption: AdoptionDraft;
  decorations: DecorationDraft[];
  selectedDecorationId: string | null;
  /** Server decoration id being optimistically moved, for reconciliation. */
  pendingDecorationIds: string[];
  settings: SettingsState;

  setAdoptionDraft: (patch: Partial<AdoptionDraft>) => void;
  resetAdoption: () => void;

  addDecoration: (item: DecorationDraft) => void;
  moveDecoration: (id: string, x: number, y: number, rotation?: number) => void;
  selectDecoration: (id: string | null) => void;
  removeDecoration: (id: string) => void;
  /** Seeds the editor from the authoritative snapshot. */
  hydrateDecorations: (items: DecorationDraft[]) => void;
  markPendingDecoration: (id: string) => void;
  resolvePendingDecoration: (id: string) => void;

  setReduceMotionOverride: (value: boolean | null) => void;
};

export const createUiStore = create<UiState>((set) => ({
  adoption: { name: '', color: '#7fd6b4', mode: null },
  decorations: [],
  selectedDecorationId: null,
  pendingDecorationIds: [],
  settings: { reduceMotionOverride: null },

  setAdoptionDraft: (patch) =>
    set((s) => ({ adoption: { ...s.adoption, ...patch } })),
  resetAdoption: () => set({ adoption: { name: '', color: '#7fd6b4', mode: null } }),

  addDecoration: (item) =>
    set((s) => ({
      decorations: [...s.decorations.filter((d) => d.id !== item.id), item],
      selectedDecorationId: item.id,
    })),
  moveDecoration: (id, x, y, rotation) =>
    set((s) => ({
      decorations: s.decorations.map((d) =>
        d.id === id ? { ...d, x, y, rotation: rotation ?? d.rotation } : d,
      ),
      selectedDecorationId: id,
    })),
  selectDecoration: (id) => set({ selectedDecorationId: id }),
  removeDecoration: (id) =>
    set((s) => ({
      decorations: s.decorations.filter((d) => d.id !== id),
      selectedDecorationId: s.selectedDecorationId === id ? null : s.selectedDecorationId,
    })),
  hydrateDecorations: (items) => set({ decorations: items, pendingDecorationIds: [] }),

  markPendingDecoration: (id) =>
    set((s) => ({
      pendingDecorationIds: s.pendingDecorationIds.includes(id)
        ? s.pendingDecorationIds
        : [...s.pendingDecorationIds, id],
    })),
  resolvePendingDecoration: (id) =>
    set((s) => ({ pendingDecorationIds: s.pendingDecorationIds.filter((p) => p !== id) })),

  setReduceMotionOverride: (value) =>
    set((s) => ({ settings: { ...s.settings, reduceMotionOverride: value } })),
}));
