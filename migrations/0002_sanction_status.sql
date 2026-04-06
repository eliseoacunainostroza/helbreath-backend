ALTER TABLE sanctions
ADD COLUMN IF NOT EXISTS status VARCHAR(16) NOT NULL DEFAULT 'active';

UPDATE sanctions
SET status = CASE
  WHEN ends_at IS NOT NULL AND ends_at < now() THEN 'expired'
  ELSE 'active'
END
WHERE status IS NULL OR status = '';

CREATE INDEX IF NOT EXISTS idx_sanctions_status ON sanctions(status);
