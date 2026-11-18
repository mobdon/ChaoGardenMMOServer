import React, { useState } from 'react';
import { Pressable, StyleSheet, Switch, Text, View } from 'react-native';
import { useRouter } from 'expo-router';
import { Screen, AppButton } from '@/components/primitives';
import { ConnectionBanner } from '@/components/ConnectionBanner';
import { connectionStore } from '@/state/connection';
import { createUiStore } from '@/state/uiStore';
import { api } from '@/api/appClient';
import { theme } from '@/theme';

/** Settings: reduced-motion override, sign out, token re-entry after expiry. */
export default function SettingsScreen() {
  const router = useRouter();
  const [reduceMotion, setReduceMotion] = useState(createUiStore.getState().settings.reduceMotionOverride ?? false);
  const override = createUiStore((s) => s.settings.reduceMotionOverride);
  const setOverride = createUiStore((s) => s.setReduceMotionOverride);

  return (
    <Screen title="Settings" testID="settings-screen">
      <ConnectionBanner store={connectionStore} />
      <View style={styles.row}>
        <Text style={styles.label} maxFontSizeMultiplier={1.5}>
          Reduce motion (overrides system)
        </Text>
        <Switch
          accessibilityLabel="Reduce motion"
          value={reduceMotion}
          onValueChange={(v) => {
            setReduceMotion(v);
            setOverride(v);
          }}
          testID="settings-reduce-motion"
        />
      </View>

      <View style={styles.actions}>
        <AppButton label="Sign out" variant="danger" onPress={async () => { await api.logout(); router.replace('/'); }} testID="settings-signout" />
      </View>
    </Screen>
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    backgroundColor: theme.colors.surface,
    borderRadius: theme.radii.m,
    padding: theme.spacing.m,
  },
  label: { ...theme.typography.body, color: theme.colors.text, flex: 1 },
  actions: { marginTop: theme.spacing.m, gap: theme.spacing.m },
});
