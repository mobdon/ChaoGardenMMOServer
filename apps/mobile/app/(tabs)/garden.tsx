import React, { useEffect } from 'react';
import { Dimensions, StyleSheet, Text, View } from 'react-native';
import { useRouter } from 'expo-router';
import { useReducedMotion } from 'react-native-reanimated';
import { Screen, AppButton } from '@/components/primitives';
import { CreatureStatusCard } from '@/components/CreatureStatusCard';
import { GardenCanvas } from '@/components/GardenCanvas';
import { LoadingView, EmptyView, ErrorView } from '@/components/StateViews';
import { ConnectionBanner } from '@/components/ConnectionBanner';
import { CareActionBar } from '@/components/CareActionBar';
import { connectionStore } from '@/state/connection';
import { createUiStore } from '@/state/uiStore';
import { useSnapshot, useCareMutation } from '@/query/hooks';
import { api } from '@/api/appClient';
import { motionConfig } from '@/motion';
import { theme } from '@/theme';

const careOptions = [
  { id: 'feed', label: 'Feed', hint: 'Open the inventory to feed your creature' },
  { id: 'pet', label: 'Pet', hint: 'Pet your creature to raise mood' },
  { id: 'clean', label: 'Clean', hint: 'Clean your creature' },
  { id: 'sleep', label: 'Sleep', hint: 'Let your creature rest' },
  { id: 'play', label: 'Play', hint: 'Play with a toy' },
];

/**
 * Garden home: animated placeholder garden plus creature status and quick care
 * actions. Reduced motion disables the entrance animation.
 */
export default function GardenScreen() {
  const router = useRouter();
  const osReduceMotion = useReducedMotion();
  const override = createUiStore((s) => s.settings.reduceMotionOverride);
  const reducedMotion = override ?? !!osReduceMotion;
  const pendingIds = createUiStore((s) => s.pendingDecorationIds);
  const hydrateDecorations = createUiStore((s) => s.hydrateDecorations);

  const snap = useSnapshot(api);
  const care = useCareMutation(snap.data?.creature?.id ?? '', api);
  const motion = motionConfig(reducedMotion);

  useEffect(() => {
    if (snap.data) hydrateDecorations(snap.data.decorations);
  }, [snap.data, hydrateDecorations]);

  const creature = snap.data?.creature ?? null;

  if (snap.isPending) {
    return <LoadingView label="Loading garden" />;
  }
  if (snap.isError) {
    const offline = snap.error instanceof Error && snap.error.name === 'OfflineError';
    return (
      <Screen title="Garden" testID="garden-screen">
        <ConnectionBanner store={connectionStore} />
        <ErrorView message={snap.error.message} offline={offline} onRetry={() => snap.refetch()} />
      </Screen>
    );
  }
  if (!creature) {
    return (
      <Screen title="Garden" testID="garden-screen">
        <ConnectionBanner store={connectionStore} />
        <EmptyView
          title="No creature yet"
          detail="Adopt a creature to start your garden."
          actionLabel="Adopt a creature"
          onAction={() => router.push('/adopt')}
        />
      </Screen>
    );
  }

  const { width } = Dimensions.get('window');
  const canvasHeight = Math.min(width * 0.9, 320);

  return (
    <Screen
      title="Garden"
      subtitle={`${creature.name} · ${snap.data?.rings ?? 0} rings`}
      testID="garden-screen"
    >
      <ConnectionBanner store={connectionStore} />
      <View style={styles.canvasWrap}>
        <GardenCanvas
          width={width - theme.spacing.m * 2}
          height={canvasHeight}
          creature={{ alignment: creature.alignment, x: (width - theme.spacing.m * 2) / 2, y: canvasHeight * 0.72, mood: creature.mood }}
          decorations={createUiStore.getState().decorations}
          pendingIds={pendingIds}
        />
      </View>
      <View style={motion.reduced ? {} : styles.cardWrap}>
        <CreatureStatusCard creature={creature} syncing={care.isPending} />
      </View>
      <View style={styles.actions}>
        <AppButton label="Open creature" variant="secondary" onPress={() => router.push(`/creature/${creature.id}`)} testID="garden-open-detail" />
        <AppButton label="Decorate" variant="secondary" onPress={() => router.push('/decorate')} testID="garden-decorate" />
        <AppButton label="Train" variant="secondary" onPress={() => router.push('/training')} testID="garden-train" />
      </View>
      <View style={styles.careWrap}>
        <QuickCare
          busy={care.isPending}
          disabled={false}
          onAction={(id) => {
            care.mutate({ kind: id as 'pet' | 'clean' | 'sleep', key: `${id}-${Date.now()}` });
          }}
        />
      </View>
    </Screen>
  );
}

/** Memoized care bar so snapshot polling does not re-render the buttons. */
const QuickCare = React.memo(function QuickCare({
  busy,
  disabled,
  onAction,
}: {
  busy: boolean;
  disabled: boolean;
  onAction: (id: string) => void;
}) {
  return <CareActionBar options={careOptions.slice(1)} onAction={onAction} busy={busy} disabled={disabled} testID="garden-care" />;
});

const styles = StyleSheet.create({
  canvasWrap: { marginBottom: theme.spacing.m },
  cardWrap: {},
  actions: { flexDirection: 'row', gap: theme.spacing.s, marginTop: theme.spacing.m, flexWrap: 'wrap' },
  careWrap: { marginTop: theme.spacing.m },
});
