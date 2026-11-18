/** Typed domain errors, decoupled from Prisma's error surface. */
export enum DomainErrorCode {
  InsufficientFunds = 'DOMAIN_INSUFFICIENT_FUNDS',
  InsufficientStock = 'DOMAIN_INSUFFICIENT_STOCK',
  DuplicateEntry = 'DOMAIN_DUPLICATE_ENTRY',
  NotFound = 'DOMAIN_NOT_FOUND',
  TileOccupied = 'DOMAIN_TILE_OCCUPIED',
  IdempotencyConflict = 'DOMAIN_IDEMPOTENCY_CONFLICT',
}

export class DomainError extends Error {
  constructor(
    public readonly code: DomainErrorCode,
    message: string,
    public readonly details?: Record<string, unknown>,
  ) {
    super(message);
    this.name = 'DomainError';
  }
}

/** Maps Prisma's unique-violation to a domain error for idempotency re-runs. */
export function isUniqueViolation(err: unknown): boolean {
  return (
    typeof err === 'object' && err !== null && 'code' in err && (err as { code: unknown }).code === 'P2002'
  );
}
