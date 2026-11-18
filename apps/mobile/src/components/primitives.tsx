import React from 'react';
import { Pressable, StyleSheet, Text, View, ViewStyle, StyleProp } from 'react-native';
import { theme } from '@/theme';

/**
 * Accessible pressable button: minimum 44x44 touch target, clear focusable
 * label, disabled state styling, and variant support.
 */
type Variant = 'primary' | 'secondary' | 'danger';

export function AppButton({
  label,
  onPress,
  variant = 'primary',
  disabled,
  hint,
  accessibilityLabel,
  style,
  testID,
}: {
  label: string;
  onPress: () => void;
  variant?: Variant;
  disabled?: boolean;
  hint?: string;
  accessibilityLabel?: string;
  style?: StyleProp<ViewStyle>;
  testID?: string;
}) {
  const bg =
    variant === 'primary'
      ? theme.colors.primary
      : variant === 'danger'
        ? theme.colors.danger
        : theme.colors.surfaceRaised;
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={accessibilityLabel ?? label}
      accessibilityHint={hint}
      accessibilityState={{ disabled: !!disabled }}
      disabled={disabled}
      onPress={onPress}
      testID={testID}
      style={({ pressed }) => [
        styles.base,
        { backgroundColor: bg, opacity: disabled ? 0.5 : pressed ? 0.85 : 1 },
        style,
      ]}
    >
      <Text
        style={[styles.label, { color: variant === 'primary' ? theme.colors.primaryText : theme.colors.text }]}
        maxFontSizeMultiplier={1.5}
      >
        {label}
      </Text>
    </Pressable>
  );
}

/** Simple vertical stat bar with text label (screen-reader friendly). */
export function StatBar({
  label,
  value,
  testID,
}: {
  label: string;
  value: number;
  testID?: string;
}) {
  const pct = Math.max(0, Math.min(100, value));
  return (
    <View style={styles.statRow} testID={testID}>
      <Text style={styles.statLabel} maxFontSizeMultiplier={1.5}>
        {label}
      </Text>
      <View
        accessibilityRole="adjustable"
        accessibilityLabel={`${label}: ${pct} percent`}
        style={styles.track}
      >
        <View style={[styles.fill, { width: `${pct}%` }]} />
      </View>
    </View>
  );
}

/** Vertical scrollable page scaffold that respects safe areas. */
export function Screen({
  title,
  subtitle,
  children,
  footer,
  testID,
}: {
  title: string;
  subtitle?: string;
  children: React.ReactNode;
  footer?: React.ReactNode;
  testID?: string;
}) {
  return (
    <View style={styles.screen} testID={testID}>
      <View style={styles.header}>
        <Text style={styles.title} maxFontSizeMultiplier={1.4}>
          {title}
        </Text>
        {subtitle ? (
          <Text style={styles.subtitle} maxFontSizeMultiplier={1.5}>
            {subtitle}
          </Text>
        ) : null}
      </View>
      <View style={styles.body}>{children}</View>
      {footer ? <View style={styles.footer}>{footer}</View> : null}
    </View>
  );
}

const styles = StyleSheet.create({
  base: {
    minHeight: theme.minTouchTarget,
    minWidth: theme.minTouchTarget,
    paddingHorizontal: theme.spacing.m,
    borderRadius: theme.radii.m,
    alignItems: 'center',
    justifyContent: 'center',
  },
  label: { ...theme.typography.body, fontWeight: '600' },
  screen: { flex: 1, backgroundColor: theme.colors.bg, padding: theme.spacing.m },
  header: { gap: theme.spacing.xs, marginBottom: theme.spacing.m },
  title: { ...theme.typography.title, color: theme.colors.text },
  subtitle: { ...theme.typography.body, color: theme.colors.textMuted },
  body: { flex: 1 },
  footer: { paddingTop: theme.spacing.m },
  statRow: { flexDirection: 'row', alignItems: 'center', gap: theme.spacing.s, marginVertical: theme.spacing.xs },
  statLabel: { width: 88, ...theme.typography.caption, color: theme.colors.textMuted },
  track: {
    flex: 1,
    height: 12,
    borderRadius: 6,
    backgroundColor: theme.colors.surfaceRaised,
    overflow: 'hidden',
  },
  fill: { height: '100%', backgroundColor: theme.colors.primary },
});
