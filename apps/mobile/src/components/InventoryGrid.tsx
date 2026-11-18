import React from 'react';
import { StyleSheet, Text, View } from 'react-native';
import type { InventoryItem } from '@chaogarden/contracts';
import { theme } from '@/theme';
import { CatalogShape } from './PlaceholderArt';
import { AppButton } from './primitives';

/**
 * Inventory grid. Quantity >0 items expose a "Use" control; the active item
 * is highlighted so screen readers announce selection.
 */
export function InventoryGrid({
  items,
  onUse,
  busy = false,
  emptyLabel = 'Inventory is empty — find items while exploring',
}: {
  items: InventoryItem[];
  onUse?: (itemId: string) => void;
  busy?: boolean;
  emptyLabel?: string;
}) {
  if (items.length === 0) {
    return (
      <View style={styles.empty} testID="inventory-empty">
        <Text style={styles.emptyText}>{emptyLabel}</Text>
      </View>
    );
  }
  return (
    <View style={styles.grid} testID="inventory-grid" accessibilityLabel="Inventory items">
      {items.map((item) => (
        <View
          key={item.id}
          style={styles.cell}
          accessibilityLabel={`${item.name}, ${item.quantity} available`}
          testID={`inventory-item-${item.id}`}
        >
          <CatalogShape catalogId={item.catalogId} size={40} />
          <Text style={styles.name} maxFontSizeMultiplier={1.5} numberOfLines={2}>
            {item.name}
          </Text>
          <Text style={styles.qty}>×{item.quantity}</Text>
          {onUse ? (
            <AppButton
              label="Use"
              variant="secondary"
              hint={`Use ${item.name}`}
              disabled={busy || item.quantity <= 0}
              onPress={() => onUse(item.id)}
              testID={`inventory-use-${item.id}`}
              style={styles.use}
            />
          ) : null}
        </View>
      ))}
    </View>
  );
}

const styles = StyleSheet.create({
  grid: { flexDirection: 'row', flexWrap: 'wrap', gap: theme.spacing.s },
  cell: {
    backgroundColor: theme.colors.surface,
    borderRadius: theme.radii.m,
    padding: theme.spacing.s,
    width: '31%',
    minWidth: 96,
    alignItems: 'center',
    gap: theme.spacing.xs,
  },
  name: { ...theme.typography.caption, color: theme.colors.text, textAlign: 'center' },
  qty: { ...theme.typography.caption, color: theme.colors.textMuted },
  use: { paddingHorizontal: theme.spacing.s, minHeight: 36 },
  empty: { padding: theme.spacing.l, alignItems: 'center' },
  emptyText: { ...theme.typography.body, color: theme.colors.textMuted, textAlign: 'center' },
});
