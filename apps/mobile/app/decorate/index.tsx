import React, { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { useRouter } from 'expo-router';
import { Screen, AppButton } from '@/components/primitives';
import { GardenCanvas, DecorationSlot } from '@/components/GardenCanvas';
import { LoadingView, ErrorView, EmptyView } from '@/components/StateViews';
import { ConnectionBanner } from '@/components/ConnectionBanner';
import { connectionStore } from '@/state/connection';
import { createUiStore, type DecorationDraft } from '@/state/uiStore';
import { useSnapshot, useMoveDecorationMutation } from '@/query/hooks';
import { api } from '@/api/appClient';
import { theme } from '@/theme';

const decorationCatalog = [
  { catalogId: 'deco_plant', name: 'Plant' },
  { catalogId: 'deco_fountain', name: 'Fountain' },
  { catalogId: 'deco_lamp', name: 'Garden lamp' },
];

/**
 * Decoration editor: place catalog items on the garden grid, drag-step their
 * position with nudge buttons (accessible alternative to dragging), then save.
 * Moves are optimistic in the Zustand draft, reconciled by the authoritative
 * PATCH response.
 */
export default function DecorateScreen() {
  const router = useRouter();
  const snap = useSnapshot(api);
  const decorations = createUiStore((s) => s.decorations);
  const selected = createUiStore((s) => s.selectedDecorationId);
  const pendingIds = createUiStore((s) => s.pendingDecorationIds);
  const move = createUiStore((s) => s.moveDecoration);
  const add = createUiStore((s) => s.addDecoration);
  const markPending = createUiStore((s) => s.markPendingDecoration);
  const resolvePending = createUiStore((s) => s.resolvePendingDecoration);
  const moveMutation = useMoveDecorationMutation(api);
  const [error, setError] = useState<string | null>(null);

  if (snap.isPending) return <LoadingView label="Loading decorations" />;
  if (snap.isError) {
    return (
      <Screen title="Decorate" testID="decorate-screen">
        <ErrorView message={snap.error.message} offline={snap.error instanceof Error && snap.error.name === 'OfflineError'} onRetry={() => snap.refetch()} />
      </Screen>
    );
  }
  if (!snap.data?.creature) {
    return (
      <Screen title="Decorate" testID="decorate-screen">
        <EmptyView title="No garden yet" detail="Adopt a creature first." actionLabel="Adopt" onAction={() => router.push('/adopt')} />
      </Screen>
    );
  }

  const selectedDeco = decorations.find((d) => d.id === selected) ?? null;

  async function save() {
    if (!selectedDeco) return;
    markPending(selectedDeco.id);
    try {
      const authoritative = await moveMutation.mutateAsync({
        id: selectedDeco.id,
        x: Math.round(selectedDeco.x),
        y: Math.round(selectedDeco.y),
        rotation: selectedDeco.rotation,
      });
      // Authoritative reconciliation: adopt the server's placement.
      createUiStore.getState().hydrateDecorations(authoritative.decorations);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Could not save placement');
    } finally {
      resolvePending(selectedDeco.id);
    }
  }

  return (
    <Screen title="Decorate" subtitle="Tap to place, then save. Nudge buttons move the selection." testID="decorate-screen">
      <ConnectionBanner store={connectionStore} />
      <View style={styles.palette}>
        {decorationCatalog.map((d) => (
          <AppButton
            key={d.catalogId}
            label={`Place ${d.name}`}
            variant="secondary"
            hint={`Place a ${d.name} at the garden centre`}
            onPress={() => {
              const id = `deco-${Date.now()}`;
              add({ id, catalogId: d.catalogId, x: 140, y: 200, rotation: 0 });
            }}
            testID={`decorate-place-${d.catalogId}`}
          />
        ))}
      </View>

      <GardenCanvas width={300} height={220} creature={null} decorations={decorations} pendingIds={pendingIds} />

      <View style={styles.slots}>
        {decorations.length === 0 ? (
          <Text style={styles.note} testID="decorate-none">
            No decorations placed yet.
          </Text>
        ) : (
          decorations.map((d) => (
            <DecorationSlot key={d.id} decoration={d} selected={d.id === selected} onSelect={createUiStore.getState().selectDecoration} />
          ))
        )}
      </View>

      {selectedDeco ? (
        <View style={styles.nudge}>
          <AppButton label="◀" variant="secondary" onPress={() => move(selectedDeco.id, selectedDeco.x - 10, selectedDeco.y)} accessibilityLabel="Move left" testID="decorate-left" style={styles.nudgeBtn} />
          <AppButton label="▶" variant="secondary" onPress={() => move(selectedDeco.id, selectedDeco.x + 10, selectedDeco.y)} accessibilityLabel="Move right" testID="decorate-right" style={styles.nudgeBtn} />
          <AppButton label="▲" variant="secondary" onPress={() => move(selectedDeco.id, selectedDeco.x, selectedDeco.y - 10)} accessibilityLabel="Move up" testID="decorate-up" style={styles.nudgeBtn} />
          <AppButton label="▼" variant="secondary" onPress={() => move(selectedDeco.id, selectedDeco.x, selectedDeco.y + 10)} accessibilityLabel="Move down" testID="decorate-down" style={styles.nudgeBtn} />
          <AppButton label="Save position" onPress={save} disabled={moveMutation.isPending} testID="decorate-save" style={styles.saveBtn} />
        </View>
      ) : null}

      {error ? (
        <Text style={styles.error} accessibilityLiveRegion="polite" testID="decorate-error">
          {error}
        </Text>
      ) : null}
    </Screen>
  );
}

const styles = StyleSheet.create({
  palette: { flexDirection: 'row', flexWrap: 'wrap', gap: theme.spacing.s, marginBottom: theme.spacing.m },
  slots: { flexDirection: 'row', flexWrap: 'wrap', gap: theme.spacing.s, marginVertical: theme.spacing.m },
  nudge: { flexDirection: 'row', flexWrap: 'wrap', gap: theme.spacing.s, alignItems: 'center' },
  nudgeBtn: { minWidth: 48 },
  saveBtn: { flexBasis: '100%' },
  note: { ...theme.typography.caption, color: theme.colors.textMuted },
  error: { ...theme.typography.caption, color: theme.colors.danger, marginTop: theme.spacing.s },
});
