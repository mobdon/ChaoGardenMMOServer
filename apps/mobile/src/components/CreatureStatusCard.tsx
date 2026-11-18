import React from 'react';
import { StyleSheet, Text, View } from 'react-native';
import type { Creature } from '@chaogarden/contracts';
import { theme } from '@/theme';
import { CreatureShape } from './PlaceholderArt';
import { StatBar } from './primitives';

/**
 * Creature status card: mood, hunger, energy and alignment, with an
 * accessibility summary so screen-reader users get one coherent readout.
 */
export function CreatureStatusCard({
  creature,
  syncing = false,
}: {
  creature: Creature;
  syncing?: boolean;
}) {
  const summary = `${creature.name}, mood ${Math.round(creature.mood)} percent, hunger ${Math.round(creature.hunger)} percent, energy ${Math.round(creature.energy)} percent`;
  return (
    <View
      style={styles.card}
      accessibilityLabel={summary}
      accessibilityRole="summary"
      testID="creature-status"
    >
      <CreatureShape alignment={creature.alignment} />
      <View style={styles.col}>
        <View style={styles.nameRow}>
          <Text style={styles.name} maxFontSizeMultiplier={1.4}>
            {creature.name}
          </Text>
          {syncing ? <Text style={styles.sync}>syncing…</Text> : null}
        </View>
        <StatBar label="Mood" value={creature.mood} testID="stat-mood" />
        <StatBar label="Hunger" value={creature.hunger} testID="stat-hunger" />
        <StatBar label="Energy" value={creature.energy} testID="stat-energy" />
        <StatBar label="Hygiene" value={creature.hygiene} testID="stat-hygiene" />
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  card: {
    backgroundColor: theme.colors.surface,
    borderRadius: theme.radii.l,
    padding: theme.spacing.m,
    gap: theme.spacing.m,
    flexDirection: 'row',
    alignItems: 'center',
  },
  col: { flex: 1, gap: theme.spacing.xs },
  nameRow: { flexDirection: 'row', alignItems: 'center', gap: theme.spacing.s },
  name: { ...theme.typography.heading, color: theme.colors.text },
  sync: { ...theme.typography.caption, color: theme.colors.accent },
});
