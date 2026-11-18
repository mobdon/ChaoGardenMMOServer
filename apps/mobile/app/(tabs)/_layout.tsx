import React from 'react';
import { StyleSheet } from 'react-native';
import { Tabs } from 'expo-router';
import { theme } from '@/theme';

/** Bottom tab navigation with 44pt targets and labeled icons-as-text. */
export default function TabsLayout() {
  return (
    <Tabs
      screenOptions={{
        headerShown: false,
        tabBarActiveTintColor: theme.colors.primary,
        tabBarInactiveTintColor: theme.colors.textMuted,
        tabBarStyle: { backgroundColor: theme.colors.surface, height: 56 + 24 },
        tabBarLabelStyle: { fontSize: 12 },
        tabBarButton: undefined,
      }}
    >
      <Tabs.Screen name="garden" options={{ title: 'Garden', tabBarAccessibilityLabel: 'Garden tab' }} />
      <Tabs.Screen name="creatures" options={{ title: 'Creatures', tabBarAccessibilityLabel: 'Creatures tab' }} />
      <Tabs.Screen name="inventory" options={{ title: 'Inventory', tabBarAccessibilityLabel: 'Inventory tab' }} />
      <Tabs.Screen name="progression" options={{ title: 'Progress', tabBarAccessibilityLabel: 'Progress tab' }} />
      <Tabs.Screen name="settings" options={{ title: 'Settings', tabBarAccessibilityLabel: 'Settings tab' }} />
    </Tabs>
  );
}
