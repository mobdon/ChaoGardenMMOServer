-- Chao Garden MMO — initial schema.
-- UUIDs via pgcrypto's gen_random_uuid(); requires PostgreSQL 13+ (built-in)
-- or `CREATE EXTENSION pgcrypto;` on older servers.

CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- CreateEnumType ------------------------------------------------------------
CREATE TYPE "ItemKind" AS ENUM ('FOOD', 'TRAINING', 'DECORATION');
CREATE TYPE "CreatureAlignment" AS ENUM ('HERO', 'NEUTRAL', 'DARK');
CREATE TYPE "FriendshipStatus" AS ENUM ('PENDING', 'ACCEPTED', 'REJECTED');

-- CreateUser ----------------------------------------------------------------
CREATE TABLE "User" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "email" TEXT,
    "passwordHash" TEXT,
    "displayName" TEXT NOT NULL,
    "guest" BOOLEAN NOT NULL DEFAULT false,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,
    "deletedAt" TIMESTAMP(3),

    CONSTRAINT "User_pkey" PRIMARY KEY ("id")
);

-- CreateTable DeviceSession -------------------------------------------------
CREATE TABLE "DeviceSession" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "userId" UUID NOT NULL,
    "deviceName" TEXT NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "lastUsedAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "revokedAt" TIMESTAMP(3),

    CONSTRAINT "DeviceSession_pkey" PRIMARY KEY ("id")
);

-- CreateTable RefreshToken --------------------------------------------------
CREATE TABLE "RefreshToken" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "hash" TEXT NOT NULL,
    "sessionId" UUID NOT NULL,
    "userId" UUID NOT NULL,
    "expiresAt" TIMESTAMP(3) NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "consumedAt" TIMESTAMP(3),

    CONSTRAINT "RefreshToken_pkey" PRIMARY KEY ("id")
);

-- CreateTable Profile -------------------------------------------------------
CREATE TABLE "Profile" (
    "userId" UUID NOT NULL,
    "title" TEXT,
    "rings" INTEGER NOT NULL DEFAULT 0,
    "level" INTEGER NOT NULL DEFAULT 1,
    "experience" INTEGER NOT NULL DEFAULT 0,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,

    CONSTRAINT "Profile_pkey" PRIMARY KEY ("userId"),
    CONSTRAINT "Profile_rings_non_negative" CHECK ("rings" >= 0)
);

-- CreateTable ItemDefinition ------------------------------------------------
CREATE TABLE "ItemDefinition" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "key" TEXT NOT NULL,
    "kind" "ItemKind" NOT NULL,
    "name" TEXT NOT NULL,
    "description" TEXT,
    "data" JSONB NOT NULL DEFAULT '{}',
    "price" INTEGER,
    "stackable" BOOLEAN NOT NULL DEFAULT true,
    "maxStack" INTEGER NOT NULL DEFAULT 99,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "ItemDefinition_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "ItemDefinition_price_non_negative" CHECK ("price" IS NULL OR "price" >= 0),
    CONSTRAINT "ItemDefinition_maxStack_positive" CHECK ("maxStack" > 0)
);

-- CreateTable InventoryStack ------------------------------------------------
CREATE TABLE "InventoryStack" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "ownerUid" UUID NOT NULL,
    "itemId" UUID NOT NULL,
    "quantity" INTEGER NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,

    CONSTRAINT "InventoryStack_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "InventoryStack_quantity_positive" CHECK ("quantity" > 0)
);

-- CreateTable Garden --------------------------------------------------------
CREATE TABLE "Garden" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "ownerUid" UUID NOT NULL,
    "name" TEXT NOT NULL,
    "theme" TEXT NOT NULL DEFAULT 'neutral',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,
    "deletedAt" TIMESTAMP(3),

    CONSTRAINT "Garden_pkey" PRIMARY KEY ("id")
);

-- CreateTable PlacedDecoration ----------------------------------------------
CREATE TABLE "PlacedDecoration" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "gardenUid" UUID NOT NULL,
    "itemId" UUID NOT NULL,
    "x" INTEGER NOT NULL,
    "y" INTEGER NOT NULL,
    "rotation" INTEGER NOT NULL DEFAULT 0,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "PlacedDecoration_pkey" PRIMARY KEY ("id")
);

-- CreateTable Creature ------------------------------------------------------
CREATE TABLE "Creature" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "ownerUid" UUID NOT NULL,
    "gardenUid" UUID,
    "name" TEXT NOT NULL,
    "speciesKey" TEXT NOT NULL,
    "alignment" "CreatureAlignment" NOT NULL DEFAULT 'NEUTRAL',
    "stats" JSONB NOT NULL DEFAULT '{}',
    "traits" JSONB NOT NULL DEFAULT '[]',
    "level" INTEGER NOT NULL DEFAULT 1,
    "experience" INTEGER NOT NULL DEFAULT 0,
    "mood" INTEGER NOT NULL DEFAULT 50,
    "hunger" INTEGER NOT NULL DEFAULT 50,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "updatedAt" TIMESTAMP(3) NOT NULL,
    "retiredAt" TIMESTAMP(3),

    CONSTRAINT "Creature_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "Creature_level_positive" CHECK ("level" > 0),
    CONSTRAINT "Creature_mood_range" CHECK ("mood" >= 0 AND "mood" <= 100),
    CONSTRAINT "Creature_hunger_range" CHECK ("hunger" >= 0 AND "hunger" <= 100)
);

-- CreateTable RingLedger ----------------------------------------------------
CREATE TABLE "RingLedger" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "ownerUid" UUID NOT NULL,
    "delta" INTEGER NOT NULL,
    "reason" TEXT NOT NULL,
    "idempotencyKey" TEXT,
    "meta" JSONB NOT NULL DEFAULT '{}',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "RingLedger_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "RingLedger_delta_nonzero" CHECK ("delta" <> 0)
);

-- CreateTable Friendship ----------------------------------------------------
CREATE TABLE "Friendship" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "requesterUid" UUID NOT NULL,
    "addresseeUid" UUID NOT NULL,
    "status" "FriendshipStatus" NOT NULL DEFAULT 'PENDING',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "respondedAt" TIMESTAMP(3),

    CONSTRAINT "Friendship_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "Friendship_not_self" CHECK ("requesterUid" <> "addresseeUid")
);

-- CreateTable GardenVisit ---------------------------------------------------
CREATE TABLE "GardenVisit" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "gardenUid" UUID NOT NULL,
    "visitorUid" UUID NOT NULL,
    "visitDay" DATE NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "GardenVisit_pkey" PRIMARY KEY ("id")
);

-- CreateTable Competition ---------------------------------------------------
CREATE TABLE "Competition" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "key" TEXT NOT NULL,
    "name" TEXT NOT NULL,
    "kind" TEXT NOT NULL,
    "startsAt" TIMESTAMP(3) NOT NULL,
    "endsAt" TIMESTAMP(3) NOT NULL,
    "prize" INTEGER NOT NULL DEFAULT 0,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "Competition_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "Competition_window" CHECK ("endsAt" > "startsAt"),
    CONSTRAINT "Competition_prize_non_negative" CHECK ("prize" >= 0)
);

-- CreateTable CompetitionEntry ----------------------------------------------
CREATE TABLE "CompetitionEntry" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "competitionUid" UUID NOT NULL,
    "creatureUid" UUID NOT NULL,
    "itemId" UUID,
    "score" INTEGER NOT NULL DEFAULT 0,
    "placement" INTEGER,
    "idempotencyKey" TEXT NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "CompetitionEntry_pkey" PRIMARY KEY ("id"),
    CONSTRAINT "CompetitionEntry_score_non_negative" CHECK ("score" >= 0)
);

-- CreateTable Notification --------------------------------------------------
CREATE TABLE "Notification" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "userId" UUID NOT NULL,
    "kind" TEXT NOT NULL,
    "payload" JSONB NOT NULL DEFAULT '{}',
    "readAt" TIMESTAMP(3),
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "Notification_pkey" PRIMARY KEY ("id")
);

-- CreateTable AuditLog ------------------------------------------------------
CREATE TABLE "AuditLog" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "userId" UUID NOT NULL,
    "action" TEXT NOT NULL,
    "targetUid" UUID,
    "meta" JSONB NOT NULL DEFAULT '{}',
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "AuditLog_pkey" PRIMARY KEY ("id")
);

-- CreateTable SimulationCheckpoint ------------------------------------------
CREATE TABLE "SimulationCheckpoint" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "gardenUid" UUID NOT NULL,
    "tick" BIGINT NOT NULL,
    "state" JSONB NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "SimulationCheckpoint_pkey" PRIMARY KEY ("id")
);

-- CreateIndex ---------------------------------------------------------------
CREATE UNIQUE INDEX "User_email_key" ON "User"("email");
CREATE INDEX "User_guest_idx" ON "User"("guest");

CREATE INDEX "DeviceSession_userId_revokedAt_idx" ON "DeviceSession"("userId", "revokedAt");

CREATE UNIQUE INDEX "RefreshToken_hash_key" ON "RefreshToken"("hash");
CREATE INDEX "RefreshToken_sessionId_idx" ON "RefreshToken"("sessionId");
CREATE INDEX "RefreshToken_userId_idx" ON "RefreshToken"("userId");

CREATE INDEX "Profile_rings_idx" ON "Profile"("rings");
CREATE INDEX "Profile_level_idx" ON "Profile"("level");

CREATE UNIQUE INDEX "ItemDefinition_key_key" ON "ItemDefinition"("key");
CREATE INDEX "ItemDefinition_kind_idx" ON "ItemDefinition"("kind");

CREATE UNIQUE INDEX "InventoryStack_ownerUid_itemId_key" ON "InventoryStack"("ownerUid", "itemId");
CREATE INDEX "InventoryStack_ownerUid_idx" ON "InventoryStack"("ownerUid");

CREATE UNIQUE INDEX "Garden_ownerUid_name_key" ON "Garden"("ownerUid", "name");
CREATE INDEX "Garden_ownerUid_idx" ON "Garden"("ownerUid");

CREATE UNIQUE INDEX "PlacedDecoration_gardenUid_x_y_key" ON "PlacedDecoration"("gardenUid", "x", "y");
CREATE INDEX "PlacedDecoration_gardenUid_idx" ON "PlacedDecoration"("gardenUid");

CREATE INDEX "Creature_ownerUid_idx" ON "Creature"("ownerUid");
CREATE INDEX "Creature_gardenUid_idx" ON "Creature"("gardenUid");
CREATE INDEX "Creature_speciesKey_idx" ON "Creature"("speciesKey");

CREATE UNIQUE INDEX "RingLedger_idempotencyKey_key" ON "RingLedger"("idempotencyKey");
CREATE INDEX "RingLedger_ownerUid_createdAt_idx" ON "RingLedger"("ownerUid", "createdAt");
CREATE INDEX "RingLedger_reason_idx" ON "RingLedger"("reason");

CREATE UNIQUE INDEX "Friendship_requesterUid_addresseeUid_key" ON "Friendship"("requesterUid", "addresseeUid");
CREATE INDEX "Friendship_addresseeUid_status_idx" ON "Friendship"("addresseeUid", "status");

CREATE UNIQUE INDEX "GardenVisit_gardenUid_visitorUid_visitDay_key" ON "GardenVisit"("gardenUid", "visitorUid", "visitDay");
CREATE INDEX "GardenVisit_visitorUid_idx" ON "GardenVisit"("visitorUid");

CREATE UNIQUE INDEX "Competition_key_key" ON "Competition"("key");
CREATE INDEX "Competition_startsAt_idx" ON "Competition"("startsAt");

CREATE UNIQUE INDEX "CompetitionEntry_idempotencyKey_key" ON "CompetitionEntry"("idempotencyKey");
CREATE INDEX "CompetitionEntry_competitionUid_placement_idx" ON "CompetitionEntry"("competitionUid", "placement");

CREATE INDEX "Notification_userId_readAt_idx" ON "Notification"("userId", "readAt");
CREATE INDEX "Notification_userId_createdAt_idx" ON "Notification"("userId", "createdAt");

CREATE INDEX "AuditLog_userId_createdAt_idx" ON "AuditLog"("userId", "createdAt");
CREATE INDEX "AuditLog_action_idx" ON "AuditLog"("action");

CREATE UNIQUE INDEX "SimulationCheckpoint_gardenUid_tick_key" ON "SimulationCheckpoint"("gardenUid", "tick");
CREATE INDEX "SimulationCheckpoint_gardenUid_tick_idx" ON "SimulationCheckpoint"("gardenUid", "tick");

-- AddForeignKey -------------------------------------------------------------
ALTER TABLE "DeviceSession" ADD CONSTRAINT "DeviceSession_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "RefreshToken" ADD CONSTRAINT "RefreshToken_sessionId_fkey" FOREIGN KEY ("sessionId") REFERENCES "DeviceSession"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "RefreshToken" ADD CONSTRAINT "RefreshToken_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

ALTER TABLE "Profile" ADD CONSTRAINT "Profile_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;

ALTER TABLE "InventoryStack" ADD CONSTRAINT "InventoryStack_ownerUid_fkey" FOREIGN KEY ("ownerUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "InventoryStack" ADD CONSTRAINT "InventoryStack_itemId_fkey" FOREIGN KEY ("itemId") REFERENCES "ItemDefinition"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

ALTER TABLE "Garden" ADD CONSTRAINT "Garden_ownerUid_fkey" FOREIGN KEY ("ownerUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;

ALTER TABLE "PlacedDecoration" ADD CONSTRAINT "PlacedDecoration_gardenUid_fkey" FOREIGN KEY ("gardenUid") REFERENCES "Garden"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "PlacedDecoration" ADD CONSTRAINT "PlacedDecoration_itemId_fkey" FOREIGN KEY ("itemId") REFERENCES "ItemDefinition"("id") ON DELETE RESTRICT ON UPDATE CASCADE;

ALTER TABLE "Creature" ADD CONSTRAINT "Creature_ownerUid_fkey" FOREIGN KEY ("ownerUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "Creature" ADD CONSTRAINT "Creature_gardenUid_fkey" FOREIGN KEY ("gardenUid") REFERENCES "Garden"("id") ON DELETE SET NULL ON UPDATE CASCADE;

ALTER TABLE "RingLedger" ADD CONSTRAINT "RingLedger_ownerUid_fkey" FOREIGN KEY ("ownerUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;

ALTER TABLE "Friendship" ADD CONSTRAINT "Friendship_requesterUid_fkey" FOREIGN KEY ("requesterUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "Friendship" ADD CONSTRAINT "Friendship_addresseeUid_fkey" FOREIGN KEY ("addresseeUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;

ALTER TABLE "GardenVisit" ADD CONSTRAINT "GardenVisit_gardenUid_fkey" FOREIGN KEY ("gardenUid") REFERENCES "Garden"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "GardenVisit" ADD CONSTRAINT "GardenVisit_visitorUid_fkey" FOREIGN KEY ("visitorUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;

ALTER TABLE "CompetitionEntry" ADD CONSTRAINT "CompetitionEntry_competitionUid_fkey" FOREIGN KEY ("competitionUid") REFERENCES "Competition"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "CompetitionEntry" ADD CONSTRAINT "CompetitionEntry_creatureUid_fkey" FOREIGN KEY ("creatureUid") REFERENCES "Creature"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "CompetitionEntry" ADD CONSTRAINT "CompetitionEntry_itemId_fkey" FOREIGN KEY ("itemId") REFERENCES "ItemDefinition"("id") ON DELETE SET NULL ON UPDATE CASCADE;

ALTER TABLE "Notification" ADD CONSTRAINT "Notification_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "AuditLog" ADD CONSTRAINT "AuditLog_userId_fkey" FOREIGN KEY ("userId") REFERENCES "User"("id") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "SimulationCheckpoint" ADD CONSTRAINT "SimulationCheckpoint_gardenUid_fkey" FOREIGN KEY ("gardenUid") REFERENCES "Garden"("id") ON DELETE CASCADE ON UPDATE CASCADE;
