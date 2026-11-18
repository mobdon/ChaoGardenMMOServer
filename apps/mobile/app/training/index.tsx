import React from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { useRouter } from 'expo-router';
import { Screen, AppButton } from '@/components/primitives';
import { LoadingView, EmptyView, ErrorView } from '@/components/StateViews';
import { ConnectionBanner } from '@/components/ConnectionBanner';
import { connectionStore } from '@/state/connection';
import { useSnapshot, useTrainingMutation } from '@/query/hooks';
import { api } from '@/api/appClient';
import { theme } from '@/theme';

const trainableStats = ['swim', 'fly', 'run', 'power', 'stamina'] as const;

/** Training: start a stat session; server authority governs outcomes. */
export default function TrainingScreen() {
  const router = useRouter();
  const snap = useSnapshot(api);
  const training = useTrainingMutation(api);
  const creature = snap.data?.creature ?? null;

  if (snap.isPending) return <LoadingView label="Loading training" />;
  if (!creature) {
    return (
      <Screen title="Training" testID="training-screen">
        <ConnectionBanner store={connectionStore} />
        <EmptyView title="No creature yet" detail="Adopt a creature before training." actionLabel="Adopt a creature" onAction={() => router.push('/adopt')} />
      </Screen>
    );
  }

  return (
    <Screen title="Training" subtitle="One session at a time; the server tracks duration." testID="training-screen">
      <ConnectionBanner store={connectionStore} />
      {snap.data?.training ? (
        <Text style={styles.active} testID="training-active">
          Session in progress: {snap.data.training.stat}
        </Text>
      ) : null}
      <View style={styles.grid}>
        {trainableStats.map((stat) => (
          <AppButton
            key={stat}
            label={`Train ${stat}`}
            variant="secondary"
            hint={`Start ${stat} training for ${creature.name}`}
            disabled={training.isPending}
            onPress={() => training.mutate({ creatureId: creature.id, stat })}
            testID={`training-${stat}`}
            style={styles.cell}
          />
        ))}
      </View>
      {training.isError ? (
        <Text style={styles.error} accessibilityLiveRegion="polite" testID="training-error">
          {training.error instanceof Error ? training.error.message : 'Training could not start'}
        </Text>
      ) : null}
      {training.isSuccess ? (
        <Text style={styles.ok} testID="training-started">Session started</Text>
      ) : null}
    </Screen>
  );
}

const styles = StyleSheet.create({
  grid: { flexDirection: 'row', flexWrap: 'wrap', gap: theme.spacing.s },
  cell: { flexBasis: '47%', minWidth: 120 },
  active: { ...theme.typography.body, color: theme.colors.accent, marginBottom: theme.spacing.m },
  error: { ...theme.typography.caption, color: theme.colors.danger, marginTop: theme.spacing.m },
  ok: { ...theme.typography.caption, color: theme.colors.primary, marginTop: theme.spacing.m },
});
