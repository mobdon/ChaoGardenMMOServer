import React, { useState } from 'react';
import { ActivityIndicator, StyleSheet, Text, TextInput, View } from 'react-native';
import { useRouter } from 'expo-router';
import { theme } from '@/theme';
import { AppButton, Screen } from '@/components/primitives';
import { ErrorView } from '@/components/StateViews';
import { api } from '@/api/appClient';
import { useQueryClient } from '@tanstack/react-query';
import { AuthExpiredError, ApiRequestError } from '@/api/client';

/**
 * Authentication: existing token login and fresh guest session. Auth expiry is
 * handled by the client's refresh flow; this screen only surfaces terminal
 * failures with a retry.
 */
export default function LoginScreen() {
  const router = useRouter();
  const queryClient = useQueryClient();
  const [token, setToken] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function signIn(kind: 'account' | 'guest') {
    setBusy(true);
    setError(null);
    try {
      await api.login(kind === 'guest' ? { guest: true } : { guest: false, token: token.trim() || undefined });
      await queryClient.invalidateQueries();
      router.replace('/adopt');
    } catch (err) {
      if (err instanceof AuthExpiredError) setError('Your session expired. Try again to refresh.');
      else if (err instanceof ApiRequestError) setError(err.error.message);
      else setError('Could not reach the garden server. Check your connection.');
    } finally {
      setBusy(false);
    }
  }

  if (busy) {
    return (
      <Screen title="Signing in" testID="login-busy">
        <ActivityIndicator color={theme.colors.primary} />
      </Screen>
    );
  }

  return (
    <Screen title="Sign in" subtitle="Use an access token from your main game profile, or continue as a guest." testID="login-screen">
      {error ? <ErrorView message={error} testID="login-error" onRetry={() => setError(null)} /> : null}
      <TextInput
        accessibilityLabel="Access token"
        value={token}
        onChangeText={setToken}
        placeholder="Access token"
        placeholderTextColor={theme.colors.textMuted}
        autoCapitalize="none"
        autoCorrect={false}
        style={styles.input}
        testID="login-token"
      />
      <View style={styles.actions}>
        <AppButton label="Sign in" hint="Sign in with the token above" onPress={() => signIn('account')} testID="login-submit" />
        <AppButton label="Use guest session" variant="secondary" onPress={() => signIn('guest')} testID="login-guest" />
      </View>
      <Text style={styles.note}>
        Tokens are stored on-device and refreshed automatically.
      </Text>
    </Screen>
  );
}

const styles = StyleSheet.create({
  input: {
    minHeight: theme.minTouchTarget,
    borderRadius: theme.radii.m,
    backgroundColor: theme.colors.surface,
    color: theme.colors.text,
    paddingHorizontal: theme.spacing.m,
    fontSize: 16,
  },
  actions: { flexDirection: 'row', gap: theme.spacing.m, marginTop: theme.spacing.m, flexWrap: 'wrap' },
  note: { ...theme.typography.caption, color: theme.colors.textMuted, marginTop: theme.spacing.m },
});
