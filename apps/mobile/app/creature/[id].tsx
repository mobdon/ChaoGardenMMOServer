import React from 'react';
import { StyleSheet, Text, View, ScrollView } from 'react-native';
import { useLocalSearchParams, useRouter } from 'expo-router';
import { Screen, AppButton, StatBar } from '@/components/primitives';
import { CreatureStatusCard } from '@/components/CreatureStatusCard';
import { CareActionBar, type CareOption } from '@/components/CareActionBar';
import { LoadingView, EmptyView, ErrorView } from '@/components/StateViews';
import { CreatureShape } from '@/components/PlaceholderArt';
import { useCreature, useCareMutation, useTrainingMutation } from '@/query/hooks';
import { api } from '@/api/appClient';
import { theme } from '@/theme';

const allStats = ['swim', 'fly', 'run', 'power', 'stamina'] as const;

const careOptions: CareOption[] = [
  { id: 'feed', label: 'Feed', hint: 'Feed your creature to reduce hunger' },
  { id: 'pet', label: 'Pet', hint: 'Pet your creature to raise mood' },
  { id: 'clean', label: 'Clean', hint: 'Clean your creature to raise hygiene' },
  { id: 'sleep', label: 'Sleep', hint: 'Let your creature sleep to restore energy' },
  { id: 'play', label: 'Play', hint: 'Play to reduce boredom' },
];

/** Creature detail: status, stats, care actions and training entry. */
export default function CreatureDetailScreen() {
  const router = useRouter();
  const params = useLocalSearchParams({ id: undefined });
  const creatureId = String(params.id ?? '');
  const creature = useCreature(creatureId, api);
  const care = useCareMutation(creatureId, api);
  const training = useTrainingMutation(api);

  if (creature.isPending) return <LoadingView label="Loading creature" />;
  if (creature.isError) {
    const offline = creature.error instanceof Error && creature.error.name === 'OfflineError';
    return (
      <Screen title="Creature" testID="creature-screen">
        <ErrorView message={creature.error.message} offline={offline} onRetry={() => creature.refetch()} />
      </Screen>
    );
  }
  const c = creature.data;
  if (!c) {
    return (
      <Screen title="Creature" testID="creature-screen">
        <EmptyView title="Not found" detail="This creature does not exist on the server." />
      </Screen>
    );
  }

  return (
    <Screen title={c.name} subtitle={`Alignment: ${c.alignment}`} testID="creature-screen">
      <ScrollView>
        <CreatureStatusCard creature={c} syncing={care.isPending} />
        <View style={styles.preview}>
          <CreatureShape alignment={c.alignment} size={96} />
        </View>
        <View style={styles.stats}>
          {c.stats.map((s) => (
            <StatBar key={s.name} label={s.name} value={s.value} testID={`stat-${s.name}`} />
          ))}
        </View>
        <CareActionBar
          options={careOptions}
          busy={care.isPending}
          onAction={(id) => care.mutate({ kind: id as 'feed' | 'pet' | 'clean' | 'sleep' | 'play', key: `${id}-${Date.now()}` })}
          testID="creature-care"
        />
        {care.isError ? (
          <Text style={styles.error} accessibilityLiveRegion="polite" testID="care-error">
            {care.error instanceof Error ? care.error.message : 'Care action failed'}
          </Text>
        ) : null}
        <View style={styles.actions}>
          <AppButton label="Train" hint="Start a stat training session" onPress={() => router.push('/training')} testID="creature-train" />
          <AppButton
            label={training.isPending ? 'Training…' : 'Quick drill: stamina'}
            variant="secondary"
            disabled={training.isPending}
            onPress={() => training.mutate({ creatureId, stat: 'stamina' })}
            testID="creature-quicktrain"
          />
        </View>
      </ScrollView>
    </Screen>
  );
}

const styles = StyleSheet.create({
  preview: { alignItems: 'center', marginTop: theme.spacing.m },
  stats: { marginVertical: theme.spacing.m },
  actions: { flexDirection: 'row', gap: theme.spacing.m, marginTop: theme.spacing.m },
  error: { ...theme.typography.caption, color: theme.colors.danger, marginTop: theme.spacing.m },
});
