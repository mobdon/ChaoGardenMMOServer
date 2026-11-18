import { validateSnapshot } from '@chaogarden/contracts';
import type { Snapshot } from '@chaogarden/contracts';
import {
  snapshotFromGame,
  syncWidgetSnapshot,
  readWidgetSnapshot,
  clearWidgetSnapshot,
  setWidgetStorage,
  resetWidgetStorage,
  WIDGET_STORAGE_KEY,
  IOS_APP_GROUP,
  ANDROID_DATASTORE_FILE,
  type WidgetStorage,
} from '../widgetSnapshot';

function makeGame(): Snapshot {
  return validateSnapshot({
    serverTime: 1_700_000_000_000,
    creature: {
      id: 'c1',
      name: 'Nimbus',
      color: 'green',
      mood: 0.9,
      hunger: 0.7,
      belly: 0.7,
      energy: 0.6,
      hygiene: 0.9,
      boredom: 0.2,
      alignment: 'neutral',
      stats: [],
      rings: 3,
    },
    inventory: [{ id: 'i1', catalogId: 'fruit', name: 'Sweet Fruit', quantity: 5 }],
    decorations: [],
    training: null,
    rings: 3,
  });
}

function memoryStorage(): WidgetStorage & { data: string | null } {
  const state: { data: string | null } = { data: null };
  return {
    get data() {
      return state.data;
    },
    set data(v: string | null) {
      state.data = v;
    },
    async write(json: string) {
      state.data = json;
    },
    async read() {
      return state.data;
    },
    async clear() {
      state.data = null;
    },
  };
}

describe('widget snapshot bridge', () => {
  afterEach(() => resetWidgetStorage());

  it('exposes the shared native storage identifiers', () => {
    expect(IOS_APP_GROUP).toBe('group.dev.chaogarden.tinygarden');
    expect(ANDROID_DATASTORE_FILE).toBe('chaogarden_widget');
    expect(WIDGET_STORAGE_KEY).toBe('widgetSnapshot');
  });

  it('builds a sanitized snapshot from game state', () => {
    const snap = snapshotFromGame(makeGame(), { now: 1_700_000_000_500 });
    expect(snap.schemaVersion).toBe(1);
    expect(snap.creature?.name).toBe('Nimbus');
    expect(snap.creature?.mood).toBe('happy');
    expect(snap.creature?.needs.map((n) => n.kind)).toEqual([
      'hunger',
      'energy',
      'fun',
      'affection',
    ]);
    // Inventory must never be serialized into the snapshot.
    expect(JSON.stringify(snap)).not.toContain('Sweet Fruit');
    expect(JSON.stringify(snap)).not.toContain('quantity');
  });

  it('writes the snapshot to shared storage after sync', async () => {
    const store = memoryStorage();
    setWidgetStorage(store);
    await syncWidgetSnapshot(makeGame(), { now: 1_700_000_000_500 });
    const parsed = JSON.parse(store.data ?? '{}');
    expect(parsed.schemaVersion).toBe(1);
    expect(parsed.serverTime).toBe(1_700_000_000_000);
  });

  it('round-trips and handles corrupted snapshots', async () => {
    const store = memoryStorage();
    setWidgetStorage(store);
    await syncWidgetSnapshot(makeGame(), { now: 1_700_000_000_500 });
    expect((await readWidgetSnapshot())?.creature?.name).toBe('Nimbus');

    store.data = '{not json';
    expect(await readWidgetSnapshot()).toBeNull();

    store.data = JSON.stringify({ schemaVersion: 99 });
    expect(await readWidgetSnapshot()).toBeNull();

    store.data = null;
    expect(await readWidgetSnapshot()).toBeNull();
  });

  it('clears the snapshot (account deletion)', async () => {
    const store = memoryStorage();
    setWidgetStorage(store);
    await syncWidgetSnapshot(makeGame(), { now: 1_700_000_000_500 });
    await clearWidgetSnapshot();
    expect(store.data).toBeNull();
  });

  it('omits the creature when the garden is empty', () => {
    const game = { ...makeGame(), creature: null };
    const snap = snapshotFromGame(game);
    expect(snap.creature).toBeNull();
  });
});
