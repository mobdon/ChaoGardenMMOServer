-- Social, friends and competitions extensions.
-- Privacy flags on Profile; blocks, reports and preset reactions.

ALTER TABLE "Profile" ADD COLUMN "friendCode" TEXT;
ALTER TABLE "Profile" ADD COLUMN "discoverable" BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE "Profile" ADD COLUMN "visitPermission" "VisitPermission" NOT NULL DEFAULT 'FRIENDS';
ALTER TABLE "Profile" ADD COLUMN "presenceVisible" BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE "Profile" ADD COLUMN "notifyFriendRequests" BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE "Profile" ADD COLUMN "notifyVisits" BOOLEAN NOT NULL DEFAULT true;
ALTER TABLE "Profile" ADD COLUMN "notifyCompetitions" BOOLEAN NOT NULL DEFAULT true;
CREATE UNIQUE INDEX "Profile_friendCode_key" ON "Profile"("friendCode");

CREATE TYPE "VisitPermission" AS ENUM ('FRIENDS', 'OPEN', 'CLOSED');

CREATE TABLE "SocialBlock" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "blockerUid" UUID NOT NULL,
    "blockedUid" UUID NOT NULL,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "SocialBlock_pkey" PRIMARY KEY ("id")
);
CREATE UNIQUE INDEX "SocialBlock_blockerUid_blockedUid_key" ON "SocialBlock"("blockerUid", "blockedUid");
CREATE INDEX "SocialBlock_blockedUid_idx" ON "SocialBlock"("blockedUid");

CREATE TABLE "Report" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "reporterUid" UUID NOT NULL,
    "targetUid" UUID NOT NULL,
    "category" TEXT NOT NULL,
    "reason" TEXT,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,

    CONSTRAINT "Report_pkey" PRIMARY KEY ("id")
);
CREATE INDEX "Report_reporterUid_createdAt_idx" ON "Report"("reporterUid", "createdAt");
CREATE INDEX "Report_targetUid_createdAt_idx" ON "Report"("targetUid", "createdAt");

CREATE TABLE "Reaction" (
    "id" UUID NOT NULL DEFAULT gen_random_uuid(),
    "senderUid" UUID NOT NULL,
    "targetUid" UUID NOT NULL,
    "reactionKey" TEXT NOT NULL,
    "creatureUid" UUID,
    "createdAt" TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP,
    "readAt" TIMESTAMP(3),

    CONSTRAINT "Reaction_pkey" PRIMARY KEY ("id")
);
CREATE INDEX "Reaction_targetUid_createdAt_idx" ON "Reaction"("targetUid", "createdAt");

ALTER TABLE "SocialBlock" ADD CONSTRAINT "SocialBlock_blockerUid_fkey" FOREIGN KEY ("blockerUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "SocialBlock" ADD CONSTRAINT "SocialBlock_blockedUid_fkey" FOREIGN KEY ("blockedUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "Report" ADD CONSTRAINT "Report_reporterUid_fkey" FOREIGN KEY ("reporterUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "Report" ADD CONSTRAINT "Report_targetUid_fkey" FOREIGN KEY ("targetUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "Reaction" ADD CONSTRAINT "Reaction_senderUid_fkey" FOREIGN KEY ("senderUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "Reaction" ADD CONSTRAINT "Reaction_targetUid_fkey" FOREIGN KEY ("targetUid") REFERENCES "Profile"("userId") ON DELETE CASCADE ON UPDATE CASCADE;
ALTER TABLE "Reaction" ADD CONSTRAINT "Reaction_creatureUid_fkey" FOREIGN KEY ("creatureUid") REFERENCES "Creature"("id") ON DELETE SET NULL ON UPDATE CASCADE;
