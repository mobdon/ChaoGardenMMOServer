import React from 'react';
import { StyleSheet, View } from 'react-native';
import { AppButton } from './primitives';
import { theme } from '@/theme';

export type CareOption = {
  id: string;
  label: string;
  hint: string;
  disabled?: boolean;
};

/**
 * Care action bar: feed, pet, clean, sleep, play. Each control meets the
 * 44pt target, exposes an accessibility hint, and reports busy state so the
 * optimistic→reconciled flow does not allow double-taps.
 */
export function CareActionBar({
  options,
  onAction,
  busy = false,
  disabled = false,
  testID = 'care-bar',
}: {
  options: CareOption[];
  onAction: (id: string) => void;
  busy?: boolean;
  disabled?: boolean;
  testID?: string;
}) {
  return (
    <View style={styles.row} testID={testID} accessibilityRole="toolbar" accessibilityLabel="Care actions">
      {options.map((opt) => (
        <AppButton
          key={opt.id}
          label={opt.label}
          hint={opt.hint}
          variant="secondary"
          disabled={disabled || busy || opt.disabled}
          onPress={() => onAction(opt.id)}
          accessibilityLabel={`${opt.label} care action`}
          testID={`care-${opt.id}`}
          style={styles.button}
        />
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  row: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: theme.spacing.s,
    justifyContent: 'space-between',
  },
  button: { flexBasis: '31%', minWidth: 100 },
});
