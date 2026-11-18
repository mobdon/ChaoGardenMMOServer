/**
 * Fixed-point integer helpers.
 *
 * The simulation must produce identical results on every platform, so we
 * avoid floating point for all gameplay math. Fractional quantities are
 * stored as integers scaled by 1_000 (three decimal digits).
 */

export const FIXED_POINT_SCALE = 1000;
export const FIXED_SHIFT = 10n; // 2^10 = 1024, used for fast scaling

/** Convert a float/unit value into fixed-point (e.g. 0.745 -> 745n). */
export function toFixed(value: number): bigint {
  return BigInt(Math.round(value * FIXED_POINT_SCALE));
}

/** Convert fixed-point back into an inexact number (display only). */
export function fromFixed(value: bigint): number {
  return Number(value) / FIXED_POINT_SCALE;
}

/** Integer division that rounds half away from zero (deterministic). */
export function divRound(num: bigint, den: bigint): bigint {
  if (den === 0n) throw new Error('fixed.divRound: division by zero');
  const neg = num < 0n !== den < 0n;
  const a = num < 0n ? -num : num;
  const b = den < 0n ? -den : den;
  let q = a / b;
  const rem = a % b;
  if (rem * 2n >= b) q += 1n;
  return neg ? -q : q;
}

/**
 * Scale a per-hour bigint delta to an elapsed duration, rounding deterministically.
 * elapsedMs is always non-negative.
 */
export function scalePerHour(deltaPerHour: bigint, elapsedMs: bigint): bigint {
  return divRound(deltaPerHour * elapsedMs, 3_600_000n);
}

export function clampBig(value: bigint, min: bigint, max: bigint): bigint {
  if (value < min) return min;
  if (value > max) return max;
  return value;
}
