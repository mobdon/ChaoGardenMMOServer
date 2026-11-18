import { Module } from '@nestjs/common';
import { DomainServices } from './domain.services';

/**
 * Domain module — wires the transactional service facade. Repositories are
 * lightweight classes instantiated by DomainServices, so only the facade is
 * registered as a provider.
 */
@Module({
  providers: [DomainServices],
  exports: [DomainServices],
})
export class DomainModule {}
