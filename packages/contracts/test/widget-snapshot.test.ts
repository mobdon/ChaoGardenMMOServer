import {
  buildWidgetSnapshot,
  assertSafeSnapshot,
  isSnapshotStale,
  isValidWidgetSnapshot,
  WIDGET_SNAPSHOT_SCHEMA_VERSION,
  WIDGET_DEEP_LINKS,
  WIDGET_FRESHNESS_MS,
} from '../src/widget-snapshot';

describe('widget snapshot contract', () => {
  const base = {
    serverTime: 1_700_000_000_000,
    now: 1_700_000_000_500,
    gardenName: 'Emerald Meadow',
    creatureName: 'Nimbus',
    creatureMood: 'happy',
    creatureThumbnail: 'creature/green',
    needs: [
      { kind: 'hunger', level: 0.8 },
      { kind: 'energy', level: 0.4 },
    ],
  };

  it('writes the current schema version and deep links', () => {
    const s = buildWidgetSnapshot(base);
    expect(s.schemaVersion).toBe(WIDGET_SNAPSHOT_SCHEMA_VERSION);
    expect(s.actions).toEqual(WIDGET_DEEP_LINKS);
    expect(s.actions.feed).toBe('chaogarden://widget/feed');
    expect(s.actions.train).toBe('chaogarden://widget/train');
    expect(s.actions.garden).toBe('chaogarden://widget/garden');
  });

  it('clamps need levels to 0..1 and truncates strings', () => {
    const s = buildWidgetSnapshot({
      ...base,
      gardenName: 'x'.repeat(100),
      creatureName: 'y'.repeat(100),
      needs: [{ kind: 'hunger', level: 42 }],
    });
    expect(s.garden.name).toHaveLength(32);
    expect(s.creature?.name).toHaveLength(24);
    expect(s.creature?.needs[0].level).toBe(1);
  });

  it('rejects unknown moods', () => {
    const s = buildWidgetSnapshot({ ...base, creatureMood: 'something-else' });
    expect(s.creature?.mood).toBe('unknown');
    expect(s.creature?.moodLabelKey).toBe('widget.mood.unknown');
  });

  it('allows a creature-less snapshot (empty garden state)', () => {
    const s = buildWidgetSnapshot({ ...base, creatureThumbnail: null });
    expect(s.creature).toBeNull();
  });

  it('sanitizer rejects forbidden data', () => {
    const s = buildWidgetSnapshot(base);
    const leaky = { ...s, accessToken: 'super-secret-token' } as never;
    expect(() => assertSafeSnapshot(leaky)).toThrow(/token/);
  });

  it('allows at most four needs', () => {
    const s = buildWidgetSnapshot({
      ...base,
      needs: [
        { kind: 'hunger', level: 1 },
        { kind: 'energy', level: 1 },
        { kind: 'fun', level: 1 },
        { kind: 'affection', level: 1 },
      ],
    });
    expect(s.creature?.needs).toHaveLength(4);
  });

  it('validates structure and schema version', () => {
    const s = buildWidgetSnapshot(base);
    expect(isValidWidgetSnapshot(JSON.parse(JSON.stringify(s)))).toBe(true);
    expect(isValidWidgetSnapshot(null)).toBe(false);
    expect(isValidWidgetSnapshot({ ...s, schemaVersion: 99 })).toBe(false);
  });

  it('flags stale snapshots past the freshness threshold', () => {
    const s = buildWidgetSnapshot(base);
    expect(isSnapshotStale(s, s.serverTime + WIDGET_FRESHNESS_MS)).toBe(false);
    expect(isSnapshotStale(s, s.serverTime + WIDGET_FRESHNESS_MS + 1)).toBe(true);
  });

  it('normalizes invalid theme colors to the default', () => {
    const s = buildWidgetSnapshot({ ...base, gardenThemeColor: 'javascript:alert(1)' });
    expect(s.garden.themeColor).toBe('#7ED957');
  });
});
