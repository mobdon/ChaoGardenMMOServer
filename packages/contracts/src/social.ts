/**
 * Social, friends, visits, reactions, notifications, competitions and privacy
 * contracts shared between the mobile app and the API.
 *
 * Every payload passes through the same hand-rolled validators used by the
 * gameplay contracts so malformed data becomes a typed error, not a crash.
 */

export type VisitPermission = 'friends' | 'open' | 'closed';

export type SocialSettings = {
  discoverable: boolean;
  visitPermission: VisitPermission;
  presenceVisible: boolean;
  notifyFriendRequests: boolean;
  notifyVisits: boolean;
  notifyCompetitions: boolean;
};

export type SocialProfile = {
  accountId: string;
  handle: string;
  friendCode: string;
  avatarKey: string | null;
  presenceVisible: boolean;
  discoverable: boolean;
  visitPermission: VisitPermission;
};

export type SocialProfileSummary = {
  accountId: string;
  handle: string;
  friendCode: string;
  avatarKey: string | null;
};

export type SocialFriend = {
  accountId: string;
  handle: string;
  friendCode: string;
  avatarKey: string | null;
  since: string;
};

export type FriendRequestStatus = 'pending' | 'friends' | 'already_friends';

export type SocialRequest = {
  id: string;
  status: 'incoming' | 'outgoing' | FriendRequestStatus;
  from?: { accountId: string; handle: string; friendCode: string };
};

export type SocialBlock = { accountId: string; handle: string };

export type ReportCategory =
  | 'harassment'
  | 'inappropriate_content'
  | 'cheating'
  | 'other';

export type ReportRequest = {
  targetId: string;
  category: ReportCategory;
  reason?: string;
};

export type ReactionKey =
  | 'wave'
  | 'cheer'
  | 'sparkle'
  | 'high_five'
  | 'heart';

/** Server-approved reaction catalog — no freeform content in the MVP. */
export const REACTION_CATALOG: Array<{ key: ReactionKey; label: string }> = [
  { key: 'wave', label: 'Wave' },
  { key: 'cheer', label: 'Cheer' },
  { key: 'sparkle', label: 'Sparkle' },
  { key: 'high_five', label: 'High five' },
  { key: 'heart', label: 'Heart' },
];

export type VisitedGarden = {
  ownerId: string;
  ownerHandle: string;
  theme: string;
  size: string;
  creatures: Array<{ id: string; name: string; mood: number; belly: number }>;
  decorations: Array<{ id: string; slot: string; catalogId: string }>;
  /** Read-only visits never allow care or decoration mutation. */
  readonly: true;
};

export type NotificationCategory =
  | 'friend_request'
  | 'visit'
  | 'reaction'
  | 'competition';

export type NotificationEnvelope = {
  id: string;
  category: NotificationCategory;
  kind: string;
  actor: { accountId: string; handle: string } | null;
  createdAt: string;
  read: boolean;
  payload: Record<string, unknown>;
};

export type CompetitionKind = 'score_attack' | 'endurance' | 'relay';

export type CompetitionRound = {
  id: string;
  kind: CompetitionKind;
  closesAt: string;
};

export type CompetitionStanding = {
  rank: number;
  accountId: string;
  handle: string;
  creatureName: string;
  score: number;
  isViewer: boolean;
};

export type CompetitionEntryResult = {
  duplicate: boolean;
  projectedScore: number;
  standings?: CompetitionStanding[];
};

export type PresenceEvent = {
  accountId: string;
  online: boolean;
  at: number;
};

export type GardenEvent =
  | { type: 'reaction'; senderId: string; reactionKey: ReactionKey }
  | { type: 'visit_start'; visitorId: string; visitId: string }
  | { type: 'visit_end'; visitorId: string; visitId: string }
  | { type: 'friend_request'; requestId: string; fromHandle: string }
  | { type: 'presence'; online: boolean };

function hasFields<T extends object>(value: unknown, fields: (keyof T)[]): value is T {
  if (typeof value !== 'object' || value === null) return false;
  return fields.every((f) => f in (value as Record<string, unknown>));
}

export function validateSocialSettings(value: unknown): SocialSettings {
  if (!hasFields<SocialSettings>(value, ['discoverable', 'visitPermission', 'presenceVisible'])) {
    throw new Error('Invalid social settings payload');
  }
  return value as SocialSettings;
}

export function validateGardenVisit(value: unknown): VisitedGarden {
  if (!hasFields<VisitedGarden>(value, ['ownerId', 'ownerHandle', 'creatures', 'readonly'])) {
    throw new Error('Invalid garden visit payload');
  }
  return value as VisitedGarden;
}

export function validateNotifications(value: unknown): NotificationEnvelope[] {
  if (!Array.isArray(value)) throw new Error('Invalid notifications payload');
  return value.map((item) => {
    if (!hasFields<NotificationEnvelope>(item, ['id', 'category', 'kind', 'createdAt', 'read', 'payload'])) {
      throw new Error('Invalid notification payload');
    }
    return item as NotificationEnvelope;
  });
}

export function validateStandings(value: unknown): CompetitionStanding[] {
  if (!Array.isArray(value)) throw new Error('Invalid standings payload');
  return value.map((item) => {
    if (!hasFields<CompetitionStanding>(item, ['rank', 'accountId', 'score'])) {
      throw new Error('Invalid standing payload');
    }
    return item as CompetitionStanding;
  });
}

/** Endpoint catalogue additions for the social surface. */
export const socialEndpoints = {
  search: (query: string) => `/api/social/profiles/search?q=${encodeURIComponent(query)}`,
  profile: (accountId: string) => `/api/social/profiles/${accountId}`,
  settings: '/api/social/settings',
  friends: '/api/social/friends',
  friend: (accountId: string) => `/api/social/friends/${accountId}`,
  requests: '/api/social/requests',
  acceptRequest: (requestId: string) => `/api/social/requests/${requestId}/accept`,
  declineRequest: (requestId: string) => `/api/social/requests/${requestId}`,
  blocks: '/api/social/blocks',
  block: (accountId: string) => `/api/social/blocks/${accountId}`,
  reports: '/api/social/reports',
  visit: (ownerId: string) => `/api/social/gardens/${ownerId}/visit`,
  garden: (ownerId: string) => `/api/social/gardens/${ownerId}`,
  react: (ownerId: string) => `/api/social/gardens/${ownerId}/react`,
  notifications: '/api/social/notifications',
  readNotifications: '/api/social/notifications/read',
  competitions: '/api/competitions',
  enterCompetition: (competitionId: string) => `/api/competitions/${competitionId}/enter`,
  competitionResults: (competitionId: string) => `/api/competitions/${competitionId}/results`,
} as const;

export const WS_EVENTS = {
  presence: 'social:presence',
  gardenEvent: 'social:garden-event',
  notification: 'social:notification',
} as const;
