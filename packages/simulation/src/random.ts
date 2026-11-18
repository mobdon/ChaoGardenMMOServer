/**
 * Deterministic pseudo random source.
 *
 * The simulation must never depend on wall-clock entropy. Everything that
 * needs randomness draws from a 64-bit xorshift-style generator seeded from
 * the creature id + checkpoint seed, so a replay reproduces identical rolls.
 */

/** SplitMix64-style mixing for seed derivation. */
function mix64(seed: bigint): bigint {
  let z = (seed + 0x9e3779b97f4a7c15n) & 0xffffffffffffffffn;
  z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & 0xffffffffffffffffn;
  z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & 0xffffffffffffffffn;
  return z ^ (z >> 31n);
}

export interface RandomSource {
  /** Next uniform integer in [0, maxExclusive). */
  nextInt(maxExclusive: number): number;
  /** Derive a sub-stream (e.g. per-activity randomness) without disturbing this one. */
  branch(label: string): RandomSource;
  /** Current state for checkpointing. */
  readonly state: bigint;
}

export function createRandomSource(seed: string | bigint): RandomSource {
  let state =
    typeof seed === 'bigint'
      ? mix64(seed)
      : mix64(hashString(seed));
  let cached: { label: string; value: bigint } | null = null;

  function hashString(s: string): bigint {
    // FNV-1a 64-bit
    let h = 0xcbf29ce484222325n;
    for (let i = 0; i < s.length; i++) {
      h ^= BigInt(s.charCodeAt(i));
      h = (h * 0x100000001b3n) & 0xffffffffffffffffn;
    }
    return h;
  }

  const source: RandomSource = {
    nextInt(maxExclusive: number): number {
      if (!Number.isInteger(maxExclusive) || maxExclusive <= 0) {
        throw new RangeError('nextInt requires a positive integer bound');
      }
      state = mix64(state);
      // Rejection-free scaling: take high bits for uniformity.
      const value = Number(state >> 32n); // 0..2^32-1
      return value % maxExclusive;
    },
    branch(label: string): RandomSource {
      const key = label;
      if (cached && cached.label === key) {
        return createRandomSource(cached.value);
      }
      const derived = mix64(state ^ hashString(key));
      cached = { label: key, value: derived };
      return createRandomSource(derived);
    },
    get state(): bigint {
      return state;
    },
  };
  return source;
}
