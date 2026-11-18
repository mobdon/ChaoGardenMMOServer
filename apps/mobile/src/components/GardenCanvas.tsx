import React, { memo } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import Canvas, { Circle, Group, Path, Rect } from '@shopify/react-native-skia';
import type { DecorationDraft } from '@/state/uiStore';
import { artFor, creatureArt } from '@/assets/registry';
import { theme } from '@/theme';

/**
 * Lightweight Skia garden renderer: sky gradient strip, ground plane, creature
 * blob, and decoration chips. Skia draws off the React tree so panning the
 * editor does not re-render screens. Falls back to plain Views in Jest via the
 * module mock.
 */
const GROUND_RATIO = 0.72;

export const GardenCanvas = memo(function GardenCanvas({
  width,
  height,
  creature,
  decorations,
  pendingIds = [],
}: {
  width: number;
  height: number;
  creature?: { alignment: string; x: number; y: number; mood: number } | null;
  decorations: DecorationDraft[];
  pendingIds?: string[];
}) {
  const groundY = height * GROUND_RATIO;
  const art = creature ? creatureArt(creature.alignment) : null;
  return (
    <View style={[styles.canvas, { width, height }]} testID="garden-canvas" accessibilityLabel="Garden view">
      <Canvas style={{ flex: 1 }}>
        <Rect x={0} y={0} width={width} height={groundY} color="#15241f" />
        <Rect x={0} y={groundY} width={width} height={height - groundY} color="#28453a" />
        {decorations.map((d) => {
          const deco = artFor(d.catalogId);
          return (
            <Group key={d.id} transform={[{ translateX: d.x }, { translateY: d.y }, { rotate: d.rotation }]}>
              <Circle cx={0} cy={0} r={16} color={deco.palette[0]} />
              <Circle cx={0} cy={0} r={7} color={deco.palette[1]} />
            </Group>
          );
        })}
        {creature && art ? (
          <Group transform={[{ translateX: creature.x }, { translateY: creature.y }]}>
            <Circle cx={0} cy={0} r={28} color={art.palette[0]} />
            <Circle cx={-10} cy={-6} r={4} color="#0b1f18" />
            <Circle cx={10} cy={-6} r={4} color="#0b1f18" />
            <Path
              path={`M -8 8 Q 0 ${creature.mood > 50 ? 16 : 4} 8 8`}
              color="#0b1f18"
              style="stroke"
              strokeWidth={2}
            />
          </Group>
        ) : null}
        {pendingIds.length > 0 ? (
          <Circle cx={width - 20} cy={20} r={6} color={theme.colors.accent} />
        ) : null}
      </Canvas>
    </View>
  );
});

const styles = StyleSheet.create({ canvas: { borderRadius: theme.radii.l, overflow: 'hidden' } });

/**
 * Touch-friendly decoration slot used by the decoration editor: tapping a
 * placed decoration selects it; long list alternative is exposed for screen
 * readers through the accessibility label.
 */
export function DecorationSlot({
  decoration,
  selected,
  onSelect,
}: {
  decoration: DecorationDraft;
  selected: boolean;
  onSelect: (id: string) => void;
}) {
  const deco = artFor(decoration.catalogId);
  return (
    <Pressable
      accessibilityRole="button"
      accessibilityLabel={`${deco.label} decoration at x ${Math.round(decoration.x)}, y ${Math.round(decoration.y)}`}
      accessibilityState={{ selected }}
      onPress={() => onSelect(decoration.id)}
      testID={`deco-${decoration.id}`}
      style={[styles.slot, selected && styles.slotSelected]}
    >
      <View style={[styles.dot, { backgroundColor: deco.palette[0] }]} />
      <Text style={styles.label} maxFontSizeMultiplier={1.5}>
        {deco.label}
      </Text>
    </Pressable>
  );
}

const styles = StyleSheet.create({
  slot: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: theme.spacing.xs,
    minHeight: theme.minTouchTarget,
    paddingHorizontal: theme.spacing.s,
    borderRadius: theme.radii.pill,
    backgroundColor: theme.colors.surface,
  },
  slotSelected: { borderWidth: 2, borderColor: theme.colors.primary },
  dot: { width: 16, height: 16, borderRadius: 8 },
  label: { color: theme.colors.text },
});
