import React, { useEffect } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { Stack } from 'expo-router';
import { SafeAreaProvider } from 'react-native-safe-area-context';
import { createQueryClient } from '@/query/hooks';
import { connectionStore } from '@/state/connection';
import { api } from '@/api/appClient';

/** Query client created once per app launch. */
const queryClient = createQueryClient();

export default function RootLayout() {
  useEffect(() => {
    // Reconcile the connection banner whenever connectivity flips back.
    const unsub = queryClient.getQueryCache().subscribe((event) => {
      if (event.type === 'success') connectionStore.getState().markSuccess();
    });
    return () => unsub();
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      <SafeAreaProvider>
        <Stack screenOptions={{ headerShown: false }}>
          <Stack.Screen name="index" />
          <Stack.Screen name="onboarding/index" />
          <Stack.Screen name="auth/login" />
          <Stack.Screen name="adopt/index" />
          <Stack.Screen name="(tabs)" />
          <Stack.Screen name="creature/[id]" />
          <Stack.Screen name="training/index" />
          <Stack.Screen name="decorate/index" />
          <Stack.Screen name="progression/index" />
        </Stack>
      </SafeAreaProvider>
    </QueryClientProvider>
  );
}

export { api };
