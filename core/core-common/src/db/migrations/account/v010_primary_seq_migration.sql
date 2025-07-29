-- Migrate primary_at (timestamp) to primary_seq (integer counter).
-- This makes the primary account selection independent of device time changes
-- while preserving the relative ordering of accounts.

-- Add the new primary_seq column
ALTER TABLE core_accounts 
ADD COLUMN primary_seq 
INTEGER NOT NULL DEFAULT 0;

-- Convert existing primary_at timestamps to sequence numbers, preserving order
UPDATE core_accounts AS a
SET primary_seq = (
  SELECT COUNT(*) 
  FROM core_accounts AS b
  WHERE b.primary_at <= a.primary_at
);

-- Drop the old primary_at column
ALTER TABLE core_accounts 
DROP COLUMN primary_at;