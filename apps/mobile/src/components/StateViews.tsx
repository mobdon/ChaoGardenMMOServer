import React from 'react';
import { ActivityIndicator, StyleSheet, Text, View } from 'react-native';
import { theme } from '@/theme';
import { AppButton } from './primitives';

/**
 * Shared loading / empty / error / offline state views with accessible labels
 * and explicit retry affordances.
 */
export function LoadingView({ label = 'Loading garden' }: { label?: string }) {
  return (
    <View style={styles.center} accessibilityRole="progressbar" accessibilityLabel={label} testID="state-loading">
      <ActivityIndicator color={theme.colors.primary} />
      <Text style={styles.text}>{label}…</Text>
    </View>
  );
}

export function EmptyView({
  title,
  detail,
  actionLabel,
  onAction,
  testID = 'state-empty',
}: {
  title: string;
  detail: string;
  actionLabel?: string;
  onAction?: () => void;
  testID?: string;
}) {
  return (
    <View style={styles.center} testID={testID} accessibilityLabel={title}>
      <Text style={styles.heading}>{title}</Text>
      <Text style={styles.text}>{detail}</Text>
      {actionLabel && onAction ? (
        <AppButton label={actionLabel} onPress={onAction} testID={`${testID}-action`} />
      ) : null}
    </View>
  );
}

export function ErrorView({
  message,
  onRetry,
  offline = false,
  testID = 'state-error',
}: {
  message: string;
  onRetry?: () => void;
  offline?: boolean;
  testID?: string;
}) {
  return (
    <View style={styles.center} testID={offline ? 'state-offline' : testID}>
      <Text style={styles.heading}>{offline ? 'You are offline' : 'Something went wrong'}</Text>
      <Text style={styles.text} accessibilityLiveRegion="polite">
        {message}
      </Text>
      {onRetry ? (
        <AppButton
          label={offline ? 'Try again' : 'Retry'}
          onPress={onRetry}
          testID={offline ? 'state-offline-retry' : 'state-error-retry'}
        />
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  center: { flex: 1, alignItems: 'center', justifyContent: 'center', gap: theme.spacing.m, padding: theme.spacing.l },
  heading: { ...theme.typography.heading, color: theme.colors.text, textAlign: 'center' },
  text: { ...theme.typography.body, color: theme.colors.textMuted, textAlign: 'center' },
});
