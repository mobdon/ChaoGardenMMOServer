import React from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { theme } from '@/theme';
import { createConnectionStore } from '@/state/connection';

const statusText: Record<string, string> = {
  online: 'Connected',
  offline: 'Offline — actions will sync when you reconnect',
  reconnecting: 'Reconnecting…',
  expired: 'Session expired — please sign in again',
};

const statusColor: Record<string, string> = {
  online: theme.colors.primary,
  offline: theme.colors.offline,
  reconnecting: theme.colors.accent,
  expired: theme.colors.danger,
};

/**
 * Persistent connection banner. Hidden when online; announced politely to
 * screen readers and rendered as plain text for assistive tech.
 */
export function ConnectionBanner({
  store = createConnectionStore,
}: {
  store?: typeof createConnectionStore;
}) {
  const status = store((s) => s.status);
  const message = store((s) => s.message);
  if (status === 'online' && !message) return null;
  return (
    <View
      accessibilityLiveRegion="polite"
      accessibilityLabel={statusText[status]}
      style={[styles.banner, { backgroundColor: statusColor[status] }]}
      testID="connection-banner"
    >
      <Text style={styles.text} maxFontSizeMultiplier={1.5}>
        {message ?? statusText[status]}
      </Text>
    </View>
  );
}

const styles = StyleSheet.create({
  banner: {
    minHeight: theme.minTouchTarget,
    paddingHorizontal: theme.spacing.m,
    paddingVertical: theme.spacing.s,
    borderRadius: theme.radii.s,
    justifyContent: 'center',
    marginBottom: theme.spacing.s,
  },
  text: { color: theme.colors.primaryText, fontWeight: '600' },
});
