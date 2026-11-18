import type { DbExecutor } from './types';

export interface NotificationView {
  id: string;
  userId: string;
  kind: string;
  payload: Record<string, unknown>;
  read: boolean;
  createdAt: Date;
}

export type NotificationCategory =
  | 'friend_request'
  | 'friend_accepted'
  | 'visit'
  | 'reaction'
  | 'competition';

export const NOTIFICATION_CATEGORIES: NotificationCategory[] = [
  'friend_request',
  'friend_accepted',
  'visit',
  'reaction',
  'competition',
];

/** Maps notification categories to the per-category opt-out profile flags. */
export function optOutFlag(category: NotificationCategory): 'notifyFriendRequests' | 'notifyVisits' | 'notifyCompetitions' | null {
  switch (category) {
    case 'friend_request':
    case 'friend_accepted':
      return 'notifyFriendRequests';
    case 'visit':
    case 'reaction':
      return 'notifyVisits';
    case 'competition':
      return 'notifyCompetitions';
    default:
      return null;
  }
}

/**
 * Notification feed. Rows are written server-side for friend requests,
 * visits, reactions and competition results and delivered over the WebSocket
 * gateway; the REST list endpoint doubles as the React Query snapshot used to
 * recover state after a reconnect.
 */
export class NotificationRepository {
  constructor(private readonly db: DbExecutor) {}

  async publish(
    input: { userId: string; category: NotificationCategory; kind: string; payload?: Record<string, unknown> },
    db: DbExecutor = this.db,
  ): Promise<NotificationView> {
    const row = await db.notification.create({
      data: {
        userId: input.userId,
        kind: input.kind,
        payload: input.payload ?? {},
      },
    });
    return {
      id: row.id,
      userId: row.userId,
      kind: row.kind,
      payload: (row.payload ?? {}) as Record<string, unknown>,
      read: row.readAt !== null,
      createdAt: row.createdAt,
    };
  }

  async list(
    userId: string,
    options: { unreadOnly?: boolean; limit?: number } = {},
    db: DbExecutor = this.db,
  ): Promise<NotificationView[]> {
    const rows = await db.notification.findMany({
      where: { userId, ...(options.unreadOnly ? { readAt: null } : {}) },
      orderBy: { createdAt: 'desc' },
      take: options.limit ?? 50,
    });
    return rows.map((row) => ({
      id: row.id,
      userId: row.userId,
      kind: row.kind,
      payload: (row.payload ?? {}) as Record<string, unknown>,
      read: row.readAt !== null,
      createdAt: row.createdAt,
    }));
  }

  async markRead(userId: string, ids: string[], db: DbExecutor = this.db): Promise<void> {
    if (ids.length === 0) return;
    await db.notification.updateMany({
      where: { userId, id: { in: ids }, readAt: null },
      data: { readAt: new Date() },
    });
  }

  async unreadCount(userId: string, db: DbExecutor = this.db): Promise<number> {
    return db.notification.count({ where: { userId, readAt: null } });
  }
}
