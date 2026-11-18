/**
 * Asset registry: the single place mapping catalog ids to original geometric
 * placeholder art specs. Replacing placeholders later only requires updating
 * `shape`/`palette` entries here (or swapping in real renderer assets) — no
 * screen code references art directly.
 *
 * All art is original simple geometry (circles, rounded rects, triangles).
 * No copyrighted characters, logos, models or sounds are used or referenced.
 */

export type ArtSpec = {
  /** Simple shape descriptor consumed by the Skia/View placeholder renderer. */
  shape: 'blob' | 'gem' | 'sphere' | 'plant' | 'fountain' | 'star' | 'cone' | 'cube' | 'pill';
  palette: [string, string];
  label: string;
};

const catalog = {
  creature_default: { shape: 'blob', palette: ['#7fd6b4', '#3d8571'], label: 'Creature' } as ArtSpec,
  creature_hero: { shape: 'blob', palette: ['#8fc4f2', '#3a6da8'], label: 'Creature (hero)' } as ArtSpec,
  creature_dark: { shape: 'blob', palette: ['#b78fd6', '#5f3a7e'], label: 'Creature (dark)' } as ArtSpec,
  food_fruit: { shape: 'sphere', palette: ['#f19a8c', '#b04a3d'], label: 'Fruit' } as ArtSpec,
  food_vegetable: { shape: 'cone', palette: ['#a9d67f', '#578a3d'], label: 'Vegetable' } as ArtSpec,
  toy_ball: { shape: 'sphere', palette: ['#f4c95d', '#b8891f'], label: 'Ball toy' } as ArtSpec,
  training_dummy: { shape: 'cube', palette: ['#a8c0b6', '#5b7168'], label: 'Training dummy' } as ArtSpec,
  deco_plant: { shape: 'plant', palette: ['#7fd6b4', '#2f6d54'], label: 'Plant' } as ArtSpec,
  deco_fountain: { shape: 'fountain', palette: ['#8fc4f2', '#33567d'], label: 'Fountain' } as ArtSpec,
  deco_lamp: { shape: 'star', palette: ['#f4c95d', '#9a6f1a'], label: 'Garden lamp' } as ArtSpec,
} as const;

export type CatalogId = keyof typeof catalog;

export function artFor(catalogId: string): ArtSpec {
  return catalog[catalogId as CatalogId] ?? catalog.creature_default;
}

export function creatureArt(alignment: string): ArtSpec {
  if (alignment === 'hero') return catalog.creature_hero;
  if (alignment === 'dark') return catalog.creature_dark;
  return catalog.creature_default;
}

export const catalogIds = Object.keys(catalog) as CatalogId[];
