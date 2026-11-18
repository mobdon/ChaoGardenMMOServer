/** Shared design tokens. Colors meet WCAG AA contrast on their paired surfaces. */
export const theme = {
  colors: {
    bg: '#101a17',
    surface: '#1a2b26',
    surfaceRaised: '#24382f',
    text: '#eef7f3',
    textMuted: '#a8c0b6',
    primary: '#7fd6b4',
    primaryText: '#0b1f18',
    accent: '#f4c95d',
    danger: '#f19a8c',
    offline: '#f4c95d',
    hero: '#8fc4f2',
    dark: '#b78fd6',
  },
  spacing: { xs: 4, s: 8, m: 16, l: 24, xl: 32 },
  radii: { s: 8, m: 14, l: 24, pill: 999 },
  /** All interactive targets meet or exceed 44x44 pt (WCAG 2.5.8 / Apple HIG). */
  minTouchTarget: 44,
  typography: {
    title: { fontSize: 26, fontWeight: '700' as const, lineHeight: 32 },
    heading: { fontSize: 20, fontWeight: '700' as const, lineHeight: 26 },
    body: { fontSize: 16, lineHeight: 22 },
    caption: { fontSize: 13, lineHeight: 18 },
  },
} as const;
