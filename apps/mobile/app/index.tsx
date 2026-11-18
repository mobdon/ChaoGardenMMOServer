import React from 'react';
import { View, Text, StyleSheet, ScrollView } from 'react-native';
import { useRouter } from 'expo-router';
import { theme } from '@/theme';
import { AppButton } from '@/components/primitives';

/**
 * Landing: choose guest onboarding or account sign-in. Guest mode uses the
 * server's guest session so the garden works before creating credentials.
 */
export default function LandingScreen() {
  const router = useRouter();
  return (
    <View style={styles.screen}>
      <View style={styles.hero}>
        <Text style={styles.title} maxFontSizeMultiplier={1.4}>
          Tiny Garden
        </Text>
        <Text style={styles.body} maxFontSizeMultiplier={1.6}>
          Raise creatures, decorate a pocket garden and visit friends. Original
          placeholder art included; real assets come from the asset catalog.
        </Text>
      </View>
      <ScrollView contentContainerStyle={styles.actions}>
        <AppButton
          label="Continue as guest"
          hint="Play immediately with a temporary garden"
          onPress={() => router.push('/onboarding')}
          testID="landing-guest"
        />
        <AppButton
          label="Sign in"
          hint="Use an existing account token"
          variant="secondary"
          onPress={() => router.push('/auth/login')}
          testID="landing-signin"
        />
      </ScrollView>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, backgroundColor: theme.colors.bg, padding: theme.spacing.l },
  hero: { flex: 1, justifyContent: 'center', gap: theme.spacing.m },
  title: { ...theme.typography.title, color: theme.colors.text, fontSize: 34 },
  body: { ...theme.typography.body, color: theme.colors.textMuted },
  actions: { gap: theme.spacing.m, paddingBottom: theme.spacing.xl },
});
