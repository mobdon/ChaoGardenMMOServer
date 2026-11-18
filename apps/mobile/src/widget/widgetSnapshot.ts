/**
 * Widget snapshot bridge.
 *
 * After a successful server synchronization the app builds a sanitized,
 * versioned snapshot and writes it into the platform-shared storage that the
 * native widgets read:
 *
 * - iOS:     `UserDefaults` in the App Group `group.dev.chaogarden.tinygarden`
 * - Android: Jetpack DataStore preferences file `chaogarden_widget`
 *
 * The native storage access is isolated behind `WidgetStorage` so tests can
 * inject a in-memory implementation and so the widget code never touches app
 * state directly.
 */

import {
  buildWidgetSnapshot,
  isSnapshotStale,
  isValidWidgetSnapshot,
  WIDGET_SNAPSHOT_SCHEMA_VERSION,
  type WidgetSnapshot,
} from '@chaogarden/contracts';
import type { Snapshot } from '@chaogarden/contracts';
import { moodFor } from './moodFor';

/** iOS App Group container id — must match the config plugin + entitlements. */
export const IOS_APP_GROUP = 'group.dev.chaogarden.tinygarden';
/** Android DataStore preferences file name. */
export const ANDROID_DATASTORE_FILE = 'chaogarden_widget';

/** Keys shared with the native readers (Swift/Kotlin constants mirror these). */
export const WIDGET_STORAGE_KEY = 'widgetSnapshot';

export interface WidgetStorage {
  /** Writes a JSON string to platform-shared storage. */
  write(json: string): Promise<void>;
  /** Reads the raw JSON string, or null when absent. */
  read(): Promise<string | null>;
  /** Removes the snapshot (e.g. after account deletion). */
  clear(): Promise<void>;
}

/** Minimal placeholder for the native module injected at build time. */
const nativeStorage: WidgetStorage = {
  async write(json) {
    const mod = await import('react-native');
    const RN = mod as unknown as {
      TinyGardenWidget?: { writeSnapshot?: (v: string) => Promise<void> };
    };
    if (RN.TinyGardenWidget?.writeSnapshot) {
      await RN.TinyGardenWidget.writeSnapshot(json);
      return;
    }
    throw new Error('TinyGardenWidget native module unavailable (requires dev build)');
  },
  async read() {
    const mod = await import('react-native');
    const RN = mod as unknown as {
      TinyGardenWidget?: { readSnapshot?: () => Promise<string | null> };
    };
    if (RN.TinyGardenWidget?.readSnapshot) {
      return RN.TinyGardenWidget.readSnapshot();
    }
    throw new Error('TinyGardenWidget native module unavailable (requires dev build)');
  },
  async clear() {
    await this.write('');
  },
};

/** Storage used by the bridge; replaceable in tests. */
let storage: WidgetStorage = nativeStorage;

/** @internal test hook */
export function setWidgetStorage(next: WidgetStorage): void {
  storage = next;
}

/** @internal test hook */
export function resetWidgetStorage(): void {
  storage = nativeStorage;
}

/**
 * Maps mood (0..1 float in the game model) to the widget mood enum.
 */

/**
 * Builds the sanitized snapshot from a validated server snapshot. Tokens and
 * inventory never enter this function's output.
 */
export function snapshotFromGame(
  game: Snapshot,
  options: { now?: number; gardenName?: string; gardenThemeColor?: string } = {},
): WidgetSnapshot {
  const creature = game.creature;
  const mood = moodFor(creature);
  return buildWidgetSnapshot({
    serverTime: game.serverTime,
    now: options.now,
    gardenName: options.gardenName ?? 'Tiny Garden',
    gardenThemeColor: options.gardenThemeColor ?? '#7ED957',
    creatureName: creature?.name ?? null,
    creatureMood: mood,
    creatureThumbnail: creature ? `creature/${creature.color}` : null,
    needs: creature
      ? [
          { kind: 'hunger', level: creature.hunger },
          { kind: 'energy', level: creature.energy },
          { kind: 'fun', level: 1 - creature.boredom },
          { kind: 'affection', level: creature.mood },
        ]
      : [],
  });
}

/**
 * Called by the app after a successful sync. Writes the snapshot and asks the
 * platform to reload widget timelines (iOS) / schedule an update (Android).
 */
export async function syncWidgetSnapshot(
  game: Snapshot,
  options: { now?: number; gardenName?: string; gardenThemeColor?: string } = {},
): Promise<void> {
  const snapshot = snapshotFromGame(game, options);
  await storage.write(JSON.stringify(snapshot));
  await reloadWidgets();
}

/** Asks native side to refresh widget timelines / respect platform quotas. */
export async function reloadWidgets(): Promise<void> {
  const mod = await import('react-native');
  const RN = mod as unknown as {
    TinyGardenWidget?: { reload?: () => Promise<void> };
  };
  if (RN.TinyGardenWidget?.reload) {
    await RN.TinyGardenWidget.reload();
  }
  // No-op in Expo Go / web: widgets require a dev build.
}

/**
 * Reads and validates the stored snapshot for the app's own widget preview.
 * Returns null when missing, corrupted, or from an unsupported schema version.
 */
export async function readWidgetSnapshot(): Promise<WidgetSnapshot | null> {
  const raw = await storage.read();
  if (!raw) return null;
  try {
    const parsed = JSON.parse(raw);
    if (!isValidWidgetSnapshot(parsed)) return null;
    return parsed;
  } catch {
    return null; // corrupted snapshot — widget shows the empty/setup state
  }
}

/** Clears widget storage (account deletion / sign-out). */
export async function clearWidgetSnapshot(): Promise<void> {
  await storage.clear();
  await reloadWidgets();
}

export { isSnapshotStale, WIDGET_SNAPSHOT_SCHEMA_VERSION };
