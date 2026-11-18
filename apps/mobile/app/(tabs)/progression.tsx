import React from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { useRouter } from 'expo-router';
import { Screen, AppButton, StatBar } from '@/components/primitives';
import { LoadingView, EmptyView, ErrorView } from '@/components/StateViews';
import { ConnectionBanner } from '@/components/ConnectionBanner';
import { connectionStore } from '@/state/connection';
import { useSnapshot } from '@/query/hooks';
import { api } from '@/api/appClient';
import { theme } from '@/theme';

/** Progression: stats and levels for the current creature. */
export default function ProgressionScreen() {
  const router = useRouter();
  const snap = useSnapshot(api);
  const creature = snap.data?.creature ?? null;

  return (
    <Screen title="Progress" subtitle={`${snap.data?.rings ?? 0} rings saved`} testID="progression-screen">
      <ConnectionBanner store={connectionStore} />
      {snap.isPending ? (
        <LoadingView label="Loading progression" />
      ) : snap.isError ? (
        <ErrorView message={snap.error.message} onRetry={() => snap.refetch()} />
      ) : !creature ? (
        <EmptyView
          title="No creature yet"
          detail="Progression begins once a creature is adopted."
          actionLabel="Adopt a creature"
          onAction={() => router.push('/adopt')}
        />
      ) : (
        <View style={styles.card}>
          {creature.stats.map((stat) => (
            <StatBar key={stat.name} label={stat.name} value={stat.value} testID={`progress-${stat.name}`} />
          ))}
          <Text style={styles.note}>Level {creature.stats.reduce((a, s) => a + s.level, 0)} total</Text>
        </View>
      )}
    </Screen>
  );
}

const styles = StyleSheet.create({
  card: {
    backgroundColor: theme.colors.surface,
    borderRadius: theme.radii.l,
    padding: theme.spacing.m,
  },
  note: { ...theme.typography.caption, color: theme.colors.textMuted, marginTop: theme.spacing.s },
});
