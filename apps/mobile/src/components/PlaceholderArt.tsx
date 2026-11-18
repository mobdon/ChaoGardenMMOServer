import React, { memo } from 'react';
import { StyleSheet, View, Text } from 'react-native';
import { artFor, creatureArt, type ArtSpec } from '@/assets/registry';
import { theme } from '@/theme';

/**
 * Original geometric placeholder renderer. Shapes are plain Views so they
 * render identically in tests; a Skia renderer can replace them later without
 * changing call sites.
 */
export const PlaceholderShape = memo(function PlaceholderShape({
  art,
  size = 48,
}: {
  art: ArtSpec;
  size?: number;
}) {
  const radius = art.shape === 'sphere' || art.shape === 'blob' ? size / 2 : size / 4;
  const isTall = art.shape === 'plant' || art.shape === 'fountain' || art.shape === 'cone';
  return (
    <View
      accessibilityLabel={art.label}
      accessibilityRole="image"
      style={{
        width: size,
        height: isTall ? size * 1.25 : size,
        borderRadius: radius,
        backgroundColor: art.palette[0],
        borderStartWidth: 3,
        borderStartColor: art.palette[1],
        justifyContent: 'center',
        alignItems: 'center',
      }}
    >
      <Text style={styles.mark} allowFontScaling={false}>
        {art.shape === 'plant' || art.shape === 'fountain' ? '▲' : '●'}
      </Text>
    </View>
  );
});

export function CatalogShape({ catalogId, size }: { catalogId: string; size?: number }) {
  return <PlaceholderShape art={artFor(catalogId)} size={size} />;
}

export function CreatureShape({
  alignment,
  size = 72,
}: {
  alignment: string;
  size?: number;
}) {
  return <PlaceholderShape art={creatureArt(alignment)} size={size} />;
}

const styles = StyleSheet.create({
  mark: { color: 'rgba(0,0,0,0.35)', fontSize: 16 },
});

/** Neutral accent used by the adoption palette selector. */
export const adoptionPalette = ['#7fd6b4', '#8fc4f2', '#b78fd6', '#f4c95d', '#f19a8c'];
export const placeholderTextColor = theme.colors.textMuted;
