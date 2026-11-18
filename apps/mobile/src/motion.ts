/**
 * Motion helpers. Every animated interaction resolves to a no-op when the
 * OS "reduce motion" accessibility setting is enabled.
 */
export function motionConfig(reducedMotion: boolean) {
  return {
    reduced: reducedMotion,
    entrance: { duration: reducedMotion ? 0 : 220, from: reducedMotion ? 1 : 0.92 },
    bounce: reducedMotion ? 0 : 1,
  };
}
