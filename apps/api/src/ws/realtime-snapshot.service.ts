/**
 * Snapshot recovery service. Snapshots are built from the same canonical
 * repositories used by REST so realtime and REST never disagree. This
 * in-memory implementation stands in for the repository-backed provider; the
 * gateway API is identical.
 */
import type { RealtimeSnapshot, VisitedGarden } from '@chaogarden/contracts';
import { PresenceService } from './presence.service';

interface StoredDecoration {
  id: string;
  slot: string;
  catalogId: string;
}

export class RealtimeSnapshotService {
  private readonly decorations = new Map<string, Array<StoredDecoration>>();
  private readonly competitions = new Map<string, { status: 'open' | 'closed'; closesAt: string | null }>();
  private readonly gardens = new Map<string, VisitedGarden>();

  lastDecoration(ownerId: string): StoredDecoration | null {
    const list = this.decorations.get(ownerId);
    return list?.length ? list[list.length - 1] : null;
  }

  moveDecoration(ownerId: string, decorationId: string, slot: string): StoredDecoration {
    const list = this.decorations.get(ownerId) ?? [];
    const next: StoredDecoration = { id: decorationId, slot, catalogId: decorationId };
    list.push(next);
    this.decorations.set(ownerId, list);
    return next;
  }

  setCompetitionStatus(competitionId: string, status: 'open' | 'closed', closesAt: string | null): void {
    this.competitions.set(competitionId, { status, closesAt });
  }

  competition(competitionId: string): { status: 'open' | 'closed'; closesAt: string | null } | null {
    return this.competitions.get(competitionId) ?? null;
  }

  /** Load the canonical garden view the client refetches after a missed-event gap. */
  garden(ownerId: string): VisitedGarden | null {
    return this.gardens.get(ownerId) ?? null;
  }

  /** Build the snapshot clients refetch after a missed-event gap. */
  build(roomId: string, unreadCount: number, currentSeq: number): RealtimeSnapshot {
    const ownerId = roomId.startsWith('garden:') ? roomId.slice('garden:'.length) : roomId;
    const competitionId = `comp:${ownerId}`;
    const competition = this.competition(competitionId);
    return {
      seq: currentSeq,
      garden: this.garden(ownerId),
      presence: [],
      unreadCount,
      competition: competition ? { id: competitionId, status: competition.status, closesAt: competition.closesAt } : null,
    };
  }

  /** Heartbeat helper shared with the presence sweep. */
  touch(presence: PresenceService, socketId: string): void {
    presence.touchHeartbeat(socketId);
  }
}
