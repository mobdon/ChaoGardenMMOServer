import React from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { useRouter } from 'expo-router';
import { Screen, AppButton } from '@/components/primitives';
import { CreatureStatusCard } from '@/components/CreatureStatusCard';
import { EmptyView } from '@/components/StateViews';
import { LoadingView, ErrorView } from '@/components/StateViews';
import { ConnectionBanner } from '@/components/ConnectionBanner';
import { connectionStore } from '@/state/connection';
import { useSnapshot } from '@/query/hooks';
import { api } from '@/api/appClient';

/** Creature roster (single-creature profiles today, list-shaped for multi). */
export default function CreaturesScreen() {
  const router = useRouter();
  const snap = useSnapshot(api);

  return (
    <Screen title="Creatures" testID="creatures-screen">
      <ConnectionBanner store={connectionStore} />
      {snap.isPending ? (
        <LoadingView label="Loading creatures" />
      ) : snap.isError ? (
        <ErrorView message={snap.error.message} onRetry={() => snap.refetch()} />
      ) : !snap.data?.creature ? (
        <EmptyView
          title="No creature yet"
          detail="Adopt your first creature to begin."
          actionLabel="Adopt a creature"
          onAction={() => router.push('/adopt')}
          testID="creatures-empty"
        />
      ) : (
        <View style={styles.list}>
          <CreatureStatusCard creature={snap.data.creature} />
          <AppButton
            label="Open details"
            hint="View stats, care actions and training"
            onPress={() => router.push(`/creature/${snap.data!.creature!.id}`)}
            testID="creatures-open"
          />
          <Text style={styles.note}>
            Stats: {snap.data.creature.stats.map((s) => `${s.name} ${s.level}`).join(' · ')}
          </Text>
        </View>
      )}
    </Screen>
  );
}

const styles = StyleSheet.create({
  list: { gap: 16 },
  note: { color: '#a8c0b6', fontSize: 13 },
});
