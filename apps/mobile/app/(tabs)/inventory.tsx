import React from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { Screen } from '@/components/primitives';
import { InventoryGrid } from '@/components/InventoryGrid';
import { LoadingView, ErrorView } from '@/components/StateViews';
import { ConnectionBanner } from '@/components/ConnectionBanner';
import { connectionStore } from '@/state/connection';
import { useSnapshot, useUseItemMutation } from '@/query/hooks';
import { api } from '@/api/appClient';
import { theme } from '@/theme';

/**
 * Inventory: browse owned items and use them. Using an item is a mutation with
 * an idempotency key; the authoritative snapshot response reconciles quantity.
 */
export default function InventoryScreen() {
  const snap = useSnapshot(api);
  const useItem = useUseItemMutation(api);

  return (
    <Screen title="Inventory" subtitle={`${snap.data?.rings ?? 0} rings`} testID="inventory-screen">
      <ConnectionBanner store={connectionStore} />
      {snap.isPending ? (
        <LoadingView label="Loading inventory" />
      ) : snap.isError ? (
        <ErrorView
          message={snap.error.message}
          offline={snap.error instanceof Error && snap.error.name === 'OfflineError'}
          onRetry={() => snap.refetch()}
        />
      ) : (
        <View>
          <InventoryGrid
            items={snap.data.inventory}
            busy={useItem.isPending}
            onUse={(itemId) => useItem.mutate({ itemId, creatureId: snap.data?.creature?.id })}
          />
          {useItem.isError ? (
            <Text style={styles.error} accessibilityLiveRegion="polite" testID="inventory-error">
              {useItem.error instanceof Error ? useItem.error.message : 'Could not use that item'}
            </Text>
          ) : null}
        </View>
      )}
    </Screen>
  );
}

const styles = StyleSheet.create({
  error: { ...theme.typography.caption, color: theme.colors.danger, marginTop: theme.spacing.m },
});
