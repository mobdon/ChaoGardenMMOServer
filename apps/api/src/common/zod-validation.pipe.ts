/**
 * Zod validation pipe. Shared contracts (`@chaogarden/contracts`) are the
 * single source of truth for request shapes on both server and client.
 * Rejections carry `AUTH_VALIDATION_ERROR` with stable field paths.
 */
import { ArgumentMetadata, Injectable, PipeTransform } from '@nestjs/common';
import { ZodSchema, z } from 'zod';
import { AuthErrorCode } from '@chaogarden/contracts';
import { AuthException } from '../auth/errors';

@Injectable()
export class ZodValidationPipe implements PipeTransform {
  constructor(private readonly schema: ZodSchema) {}

  transform(value: unknown, _metadata: ArgumentMetadata) {
    const result = this.schema.safeParse(value);
    if (!result.success) {
      const first = result.error.issues[0];
      const path = first ? first.path.join('.') : 'body';
      throw new AuthException(
        AuthErrorCode.ValidationError,
        `Invalid field: ${path}`,
      );
    }
    return result.data;
  }
}

export function validateWith<T>(schema: z.ZodType<T>, value: unknown): T {
  const result = schema.safeParse(value);
  if (!result.success) {
    const first = result.error.issues[0];
    throw new AuthException(
      AuthErrorCode.ValidationError,
      `Invalid field: ${first ? first.path.join('.') : 'body'}`,
    );
  }
  return result.data;
}
