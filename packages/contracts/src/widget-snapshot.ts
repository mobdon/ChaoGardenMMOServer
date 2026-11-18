/**
 * Versioned widget snapshot schema shared by the mobile app and the native
 * widgets (iOS WidgetKit extension / Android Glance AppWidget).
 *
 * The snapshot is deliberately tiny and sanitized: it must never contain
 * access tokens, private profile data, or full inventory data — only what the
 * widget needs to render a compact garden scene.
 */

export const WIDGET_SNAPSHOT_SCHEMA_VERSION = 1 as const;

export const WIDGET_FRESHNESS_MS = 30 * 60 * 1000; // 30 minutes

/** Mood values mirrored natively (Swift/Kotlin enums). */
export const WIDGET_MOODS = [
  'happy',
  'content',
  'hungry',
  'sleepy',
  'lonely',
  'unknown',
] as const;
export type WidgetMood = (typeof WIDGET_MOODS)[number];

export type WidgetNeedKind = 'hunger' | 'energy' | 'fun' | 'affection';

export interface WidgetNeed {
  kind: WidgetNeedKind;
  /** 0..1, rounded to 2 decimals. */
  level: number;
  /** Localization key resolved natively, never raw server text. */
  labelKey: string;
}

export interface WidgetSnapshotActions {
  garden: string;
  feed: string;
  train: string;
}

export interface WidgetSnapshotCreature {
  /** Display name, truncated to 24 chars. */
  name: string;
  mood: WidgetMood;
  /** Localization key for the mood label. */
  moodLabelKey: string;
  /** Asset key for the original creature thumbnail, e.g. "creature/nimbus". */
  thumbnail: string;
  /** At most 4 need indicators. */
  needs: WidgetNeed[];
}

export interface WidgetSnapshot {
  schemaVersion: typeof WIDGET_SNAPSHOT_SCHEMA_VERSION;
  /** Epoch ms when the app wrote the snapshot. */
  writtenAt: number;
  /** Epoch ms of the server state the snapshot reflects. */
  serverTime: number;
  garden: {
    /** Truncated to 32 chars; the only free-form server string allowed. */
    name: string;
    /** Hex color for the garden background. */
    themeColor: string;
  };
  creature: WidgetSnapshotCreature | null;
  actions: WidgetSnapshotActions;
}

/** Strings in the widget that must never appear (secret scanning on write). */
const FORBIDDEN_KEY_FRAGMENTS = ['token', 'secret', 'password', 'session', 'email'];

function clamp01(value: unknown): number {
  const n = typeof value === 'number' && Number.isFinite(value) ? value : 0;
  return Math.round(Math.min(1, Math.max(0, n)) * 100) / 100;
}

function safeString(value: unknown, maxLength: number): string {
  if (typeof value !== 'string') return '';
  // Strip control characters and hard-limit length.
  return value.replace(/[\u0000-\u001f\u007f]/g, '').slice(0, maxLength);
}

const HEX_COLOR = /^#[0-9a-fA-F]{6}$/;

/** Deep links into the app; the widget itself performs no network calls. */
export const WIDGET_DEEP_LINKS: WidgetSnapshotActions = {
  garden: 'chaogarden://widget/garden',
  feed: 'chaogarden://widget/feed',
  train: 'chaogarden://widget/train',
};

/**
 * Builds a sanitized snapshot from arbitrary game state. Throws if the result
 * would embed forbidden data (defense in depth for future schema changes).
 */
export function buildWidgetSnapshot(input: {
  serverTime: number;
  now?: number;
  gardenName?: unknown;
  gardenThemeColor?: unknown;
  creatureName?: unknown;
  creatureMood?: unknown;
  creatureThumbnail?: unknown;
  needs?: Array<{ kind?: unknown; level?: unknown }>;
}): WidgetSnapshot {
  const now = input.now ?? Date.now();
  const mood = WIDGET_MOODS.includes(input.creatureMood as WidgetMood)
    ? (input.creatureMood as WidgetMood)
    : 'unknown';

  const theme = typeof input.gardenThemeColor === 'string' && HEX_COLOR.test(input.gardenThemeColor)
    ? input.gardenThemeColor
    : '#7ED957';

  const needs = (Array.isArray(input.needs) ? input.needs : [])
    .slice(0, 4)
    .map((need) => ({
      kind: safeString(need.kind, 16) as WidgetNeedKind,
      level: clamp01(need.level),
      labelKey: `widget.need.${safeString(need.kind, 16)}`,
    }))
    .filter((need) => ['hunger', 'energy', 'fun', 'affection'].includes(need.kind));

  const snapshot: WidgetSnapshot = {
    schemaVersion: WIDGET_SNAPSHOT_SCHEMA_VERSION,
    writtenAt: now,
    serverTime: Number.isFinite(input.serverTime) ? Math.floor(input.serverTime) : now,
    garden: {
      name: safeString(input.gardenName, 32),
      themeColor: theme,
    },
    creature:
      input.creatureThumbnail == null
        ? null
        : {
            name: safeString(input.creatureName, 24),
            mood,
            moodLabelKey: `widget.mood.${mood}`,
            thumbnail: safeString(input.creatureThumbnail, 48),
            needs,
          },
    actions: WIDGET_DEEP_LINKS,
  };

  assertSafeSnapshot(snapshot);
  return snapshot;
}

/** Throws when the snapshot contains anything that must not reach the widget. */
export function assertSafeSnapshot(snapshot: WidgetSnapshot): void {
  if (snapshot.schemaVersion !== WIDGET_SNAPSHOT_SCHEMA_VERSION) {
    throw new Error(`Unsupported widget snapshot schema: ${snapshot.schemaVersion}`);
  }
  const serialized = JSON.stringify(snapshot).toLowerCase();
  for (const fragment of FORBIDDEN_KEY_FRAGMENTS) {
    if (serialized.includes(fragment)) {
      throw new Error(`Widget snapshot must not contain "${fragment}" data`);
    }
  }
  if (snapshot.creature && snapshot.creature.needs.length > 4) {
    throw new Error('Widget snapshot may contain at most 4 needs');
  }
}

/** True when the snapshot is structurally valid for the given schema version. */
export function isValidWidgetSnapshot(value: unknown): value is WidgetSnapshot {
  if (typeof value !== 'object' || value === null) return false;
  const s = value as WidgetSnapshot;
  if (s.schemaVersion !== WIDGET_SNAPSHOT_SCHEMA_VERSION) return false;
  if (typeof s.writtenAt !== 'number' || typeof s.serverTime !== 'number') return false;
  if (typeof s.garden?.name !== 'string' || typeof s.garden?.themeColor !== 'string') return false;
  if (s.creature !== null && typeof s.creature !== 'object') return false;
  if (typeof s.actions?.garden !== 'string') return false;
  return true;
}

/** Whether the snapshot has aged past the freshness threshold. */
export function isSnapshotStale(snapshot: WidgetSnapshot, now: number): boolean {
  return now - snapshot.serverTime > WIDGET_FRESHNESS_MS;
}
