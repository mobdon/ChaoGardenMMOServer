import React from 'react';
import { View, Text, StyleSheet, Pressable, TextInput } from 'react-native';
import { useRouter } from 'expo-router';
import { theme } from '@/theme';
import { AppButton, Screen } from '@/components/primitives';
import { createUiStore } from '@/state/uiStore';
import { adoptionPalette } from '@/components/PlaceholderArt';

/**
 * Onboarding: pick a creature name and color, then continue to adoption.
 * Draft state lives in the Zustand UI store (local only, not server state).
 */
export default function OnboardingScreen() {
  const router = useRouter();
  const adoption = createUiStore((s) => s.adoption);
  const setAdoptionDraft = createUiStore((s) => s.setAdoptionDraft);

  const canContinue = adoption.name.trim().length >= 2;

  return (
    <Screen
      title="Welcome to your garden"
      subtitle="Choose how your creature will look. You can change its name later."
      testID="onboarding-screen"
    >
      <Text style={styles.label}>Creature name</Text>
      <TextInput
        accessibilityLabel="Creature name"
        value={adoption.name}
        onChangeText={(name) => setAdoptionDraft({ name })}
        placeholder="e.g. Pip"
        placeholderTextColor={theme.colors.textMuted}
        style={styles.input}
        testID="onboarding-name"
        maxLength={16}
      />

      <Text style={styles.label}>Color</Text>
      <View style={styles.palette}>
        {adoptionPalette.map((color) => (
          <Pressable
            key={color}
            accessibilityRole="button"
            accessibilityLabel={`Color option ${color}`}
            accessibilityState={{ selected: adoption.color === color }}
            onPress={() => setAdoptionDraft({ color })}
            testID={`onboarding-color-${color.replace('#', '')}`}
            style={[styles.swatch, { backgroundColor: color }, adoption.color === color && styles.swatchSelected]}
          />
        ))}
      </View>

      <View style={styles.footer}>
        <AppButton
          label="Continue"
          hint="Go to creature adoption"
          disabled={!canContinue}
          onPress={() => router.push('/adopt')}
          testID="onboarding-continue"
        />
      </View>
    </Screen>
  );
}

const styles = StyleSheet.create({
  label: { ...theme.typography.body, color: theme.colors.text, marginBottom: theme.spacing.xs, marginTop: theme.spacing.m },
  input: {
    minHeight: theme.minTouchTarget,
    borderRadius: theme.radii.m,
    backgroundColor: theme.colors.surface,
    color: theme.colors.text,
    paddingHorizontal: theme.spacing.m,
    fontSize: 16,
  },
  palette: { flexDirection: 'row', gap: theme.spacing.m, flexWrap: 'wrap' },
  swatch: { width: 48, height: 48, borderRadius: 24 },
  swatchSelected: { borderWidth: 3, borderColor: theme.colors.text },
  footer: { marginTop: theme.spacing.xl },
});
