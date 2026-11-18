import React from 'react';
import { ActivityIndicator, View, Text, StyleSheet } from 'react-native';
import { useRouter } from 'expo-router';
import { theme } from '@/theme';
import { AppButton, Screen } from '@/components/primitives';
import { ErrorView } from '@/components/StateViews';
import { CreatureShape } from '@/components/PlaceholderArt';
import { createUiStore } from '@/state/uiStore';
import { useAdoptionMutation } from '@/query/hooks';
import { api } from '@/api/appClient';

/**
 * Creature adoption: confirms the onboarding draft and creates the creature.
 * Uses an idempotency key so a double-tap or retry cannot create duplicates.
 */
export default function AdoptScreen() {
  const router = useRouter();
  const adoption = createUiStore((s) => s.adoption);
  const adopt = useAdoptionMutation(api);

  if (adopt.isPending) {
    return (
      <Screen title="Hatching…" testID="adopt-busy">
        <View style={styles.busy}>
          <CreatureShape alignment="neutral" size={96} />
          <ActivityIndicator color={theme.colors.primary} />
        </View>
      </Screen>
    );
  }

  if (adopt.isError) {
    return (
      <Screen title="Adoption" testID="adopt-screen">
        <ErrorView
          message={adopt.error instanceof Error ? adopt.error.message : 'Adoption failed'}
          onRetry={() => adopt.reset()}
          offline={adopt.error instanceof Error && adopt.error.name === 'OfflineError'}
        />
      </Screen>
    );
  }

  return (
    <Screen title="Adopt your creature" subtitle="Original placeholder art shown; real assets arrive via the asset catalog." testID="adopt-screen">
      <View style={styles.preview}>
        <CreatureShape alignment="neutral" size={120} />
        <Text style={styles.name} maxFontSizeMultiplier={1.4}>
          {adoption.name || 'Your creature'}
        </Text>
      </View>
      <AppButton
        label="Adopt"
        hint="Create this creature and enter the garden"
        disabled={adoption.name.trim().length < 2}
        onPress={async () => {
          try {
            const snap = await adopt.mutateAsync({ name: adoption.name.trim(), color: adoption.color });
            createUiStore.getState().resetAdoption();
            await router.replace('/(tabs)/garden');
          } catch {
            /* error state rendered above */
          }
        }}
        testID="adopt-submit"
      />
      <AppButton label="Back" variant="secondary" onPress={() => router.back()} testID="adopt-back" />
    </Screen>
  );
}

const styles = StyleSheet.create({
  preview: { alignItems: 'center', gap: theme.spacing.s, marginBottom: theme.spacing.l },
  name: { ...theme.typography.heading, color: theme.colors.text },
  busy: { alignItems: 'center', gap: theme.spacing.m },
});
