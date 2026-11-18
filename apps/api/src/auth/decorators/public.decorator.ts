import { SetMetadata } from '@nestjs/common';

/** Marks a route/handler as skipping JWT auth (e.g. login, refresh). */
export const IS_PUBLIC_KEY = 'chaogarden.isPublic';
export const Public = () => SetMetadata(IS_PUBLIC_KEY, true);
