CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TABLE IF NOT EXISTS accounts (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  username VARCHAR(32) NOT NULL UNIQUE,
  email VARCHAR(190) UNIQUE,
  password_hash TEXT NOT NULL,
  status VARCHAR(16) NOT NULL DEFAULT 'active',
  failed_login_count INT NOT NULL DEFAULT 0,
  last_login_at TIMESTAMPTZ,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_accounts_status ON accounts(status);

CREATE TABLE IF NOT EXISTS sessions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  account_id UUID REFERENCES accounts(id) ON DELETE CASCADE,
  character_id UUID,
  gateway_node VARCHAR(64) NOT NULL,
  remote_ip INET,
  protocol_version VARCHAR(16) NOT NULL DEFAULT 'unknown',
  state VARCHAR(24) NOT NULL,
  issued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  last_seen_at TIMESTAMPTZ,
  expires_at TIMESTAMPTZ NOT NULL,
  closed_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_sessions_account_id ON sessions(account_id);
CREATE INDEX IF NOT EXISTS idx_sessions_state ON sessions(state);
CREATE INDEX IF NOT EXISTS idx_sessions_expires_at ON sessions(expires_at);

CREATE TABLE IF NOT EXISTS maps (
  id INT PRIMARY KEY,
  code VARCHAR(64) NOT NULL UNIQUE,
  name VARCHAR(128) NOT NULL,
  width INT NOT NULL DEFAULT 2048,
  height INT NOT NULL DEFAULT 2048,
  default_spawn_x INT NOT NULL DEFAULT 100,
  default_spawn_y INT NOT NULL DEFAULT 100,
  tick_ms INT NOT NULL DEFAULT 50,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS map_instances (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  map_id INT NOT NULL REFERENCES maps(id),
  shard_code VARCHAR(64) NOT NULL,
  status VARCHAR(16) NOT NULL DEFAULT 'active',
  started_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  stopped_at TIMESTAMPTZ,
  UNIQUE (map_id, shard_code)
);

CREATE TABLE IF NOT EXISTS characters (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  account_id UUID NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  name VARCHAR(16) NOT NULL UNIQUE,
  slot SMALLINT NOT NULL,
  class_code VARCHAR(16) NOT NULL DEFAULT 'warrior',
  gender SMALLINT NOT NULL DEFAULT 0,
  skin_color SMALLINT NOT NULL DEFAULT 0,
  hair_style SMALLINT NOT NULL DEFAULT 0,
  hair_color SMALLINT NOT NULL DEFAULT 0,
  underwear_color SMALLINT NOT NULL DEFAULT 0,
  level INT NOT NULL DEFAULT 1,
  exp BIGINT NOT NULL DEFAULT 0,
  map_id INT NOT NULL REFERENCES maps(id),
  pos_x INT NOT NULL DEFAULT 0,
  pos_y INT NOT NULL DEFAULT 0,
  hp INT NOT NULL DEFAULT 100,
  mp INT NOT NULL DEFAULT 50,
  sp INT NOT NULL DEFAULT 50,
  str_stat SMALLINT NOT NULL DEFAULT 10,
  vit_stat SMALLINT NOT NULL DEFAULT 10,
  dex_stat SMALLINT NOT NULL DEFAULT 10,
  int_stat SMALLINT NOT NULL DEFAULT 10,
  mag_stat SMALLINT NOT NULL DEFAULT 10,
  chr_stat SMALLINT NOT NULL DEFAULT 10,
  is_deleted BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE (account_id, slot)
);

CREATE INDEX IF NOT EXISTS idx_characters_account_id ON characters(account_id);
CREATE INDEX IF NOT EXISTS idx_characters_map_id ON characters(map_id);
CREATE INDEX IF NOT EXISTS idx_characters_name ON characters(name);

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_constraint
    WHERE conname = 'fk_sessions_character'
  ) THEN
    ALTER TABLE sessions
      ADD CONSTRAINT fk_sessions_character
      FOREIGN KEY (character_id) REFERENCES characters(id) ON DELETE SET NULL;
  END IF;
END
$$;

CREATE TABLE IF NOT EXISTS entities (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  map_instance_id UUID REFERENCES map_instances(id),
  entity_kind VARCHAR(24) NOT NULL,
  owner_character_id UUID REFERENCES characters(id),
  npc_id UUID,
  pos_x INT NOT NULL DEFAULT 0,
  pos_y INT NOT NULL DEFAULT 0,
  hp INT NOT NULL DEFAULT 1,
  mp INT NOT NULL DEFAULT 0,
  alive BOOLEAN NOT NULL DEFAULT true,
  state_json JSONB NOT NULL DEFAULT '{}'::jsonb,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_entities_map_instance_id ON entities(map_instance_id);
CREATE INDEX IF NOT EXISTS idx_entities_kind ON entities(entity_kind);

CREATE TABLE IF NOT EXISTS npcs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  code VARCHAR(64) NOT NULL UNIQUE,
  name VARCHAR(128) NOT NULL,
  behavior_tree VARCHAR(128) NOT NULL DEFAULT 'default',
  base_level INT NOT NULL DEFAULT 1,
  config_json JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS items (
  id BIGSERIAL PRIMARY KEY,
  code VARCHAR(64) NOT NULL UNIQUE,
  item_type VARCHAR(32) NOT NULL,
  max_stack INT NOT NULL DEFAULT 1,
  attrs JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS inventories (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  character_id UUID NOT NULL UNIQUE REFERENCES characters(id) ON DELETE CASCADE,
  gold BIGINT NOT NULL DEFAULT 0,
  version BIGINT NOT NULL DEFAULT 0,
  updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS inventory_items (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  inventory_id UUID NOT NULL REFERENCES inventories(id) ON DELETE CASCADE,
  item_id BIGINT NOT NULL REFERENCES items(id),
  slot INT NOT NULL,
  quantity INT NOT NULL DEFAULT 1,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  UNIQUE (inventory_id, slot)
);

CREATE TABLE IF NOT EXISTS equipment_slots (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  character_id UUID NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
  slot_code VARCHAR(32) NOT NULL,
  item_id BIGINT REFERENCES items(id),
  durability INT NOT NULL DEFAULT 0,
  metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
  UNIQUE (character_id, slot_code)
);

CREATE TABLE IF NOT EXISTS skills (
  id INT PRIMARY KEY,
  code VARCHAR(64) NOT NULL UNIQUE,
  display_name VARCHAR(128) NOT NULL,
  mana_cost INT NOT NULL DEFAULT 0,
  cooldown_ms INT NOT NULL DEFAULT 0,
  config_json JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE IF NOT EXISTS character_skills (
  character_id UUID NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
  skill_id INT NOT NULL REFERENCES skills(id),
  level SMALLINT NOT NULL DEFAULT 1,
  exp BIGINT NOT NULL DEFAULT 0,
  PRIMARY KEY (character_id, skill_id)
);

CREATE TABLE IF NOT EXISTS combat_logs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  map_instance_id UUID REFERENCES map_instances(id),
  attacker_entity_id UUID,
  defender_entity_id UUID,
  skill_id INT,
  damage INT NOT NULL,
  was_critical BOOLEAN NOT NULL DEFAULT false,
  payload JSONB NOT NULL DEFAULT '{}'::jsonb,
  occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS guilds (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name VARCHAR(32) NOT NULL UNIQUE,
  notice TEXT NOT NULL DEFAULT '',
  created_by UUID REFERENCES characters(id),
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS guild_members (
  guild_id UUID NOT NULL REFERENCES guilds(id) ON DELETE CASCADE,
  character_id UUID NOT NULL REFERENCES characters(id) ON DELETE CASCADE,
  rank VARCHAR(16) NOT NULL,
  joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  PRIMARY KEY (guild_id, character_id)
);

CREATE TABLE IF NOT EXISTS mail_messages (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  from_character_id UUID REFERENCES characters(id),
  to_character_id UUID NOT NULL REFERENCES characters(id),
  subject VARCHAR(128) NOT NULL,
  body TEXT NOT NULL,
  attached_item JSONB,
  is_read BOOLEAN NOT NULL DEFAULT false,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS game_events (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  event_type VARCHAR(64) NOT NULL,
  actor_character_id UUID REFERENCES characters(id),
  map_id INT REFERENCES maps(id),
  payload JSONB NOT NULL,
  occurred_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS sanctions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  account_id UUID REFERENCES accounts(id),
  character_id UUID REFERENCES characters(id),
  sanction_type VARCHAR(32) NOT NULL,
  reason TEXT NOT NULL,
  starts_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  ends_at TIMESTAMPTZ,
  issued_by_admin_id UUID,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS admin_users (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  email VARCHAR(190) NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  status VARCHAR(16) NOT NULL DEFAULT 'active',
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS admin_sessions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  admin_user_id UUID NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
  token_hash TEXT NOT NULL,
  issued_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  expires_at TIMESTAMPTZ NOT NULL,
  revoked_at TIMESTAMPTZ
);

CREATE TABLE IF NOT EXISTS admin_roles (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  code VARCHAR(32) NOT NULL UNIQUE,
  name VARCHAR(64) NOT NULL
);

CREATE TABLE IF NOT EXISTS admin_permissions (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  code VARCHAR(64) NOT NULL UNIQUE,
  description TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS admin_role_permissions (
  role_id UUID NOT NULL REFERENCES admin_roles(id) ON DELETE CASCADE,
  permission_id UUID NOT NULL REFERENCES admin_permissions(id) ON DELETE CASCADE,
  PRIMARY KEY (role_id, permission_id)
);

CREATE TABLE IF NOT EXISTS admin_user_roles (
  admin_user_id UUID NOT NULL REFERENCES admin_users(id) ON DELETE CASCADE,
  role_id UUID NOT NULL REFERENCES admin_roles(id) ON DELETE CASCADE,
  PRIMARY KEY (admin_user_id, role_id)
);

CREATE TABLE IF NOT EXISTS admin_audit_logs (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  admin_user_id UUID REFERENCES admin_users(id),
  action_type VARCHAR(64) NOT NULL,
  target_type VARCHAR(64) NOT NULL,
  target_id VARCHAR(128) NOT NULL,
  request_id VARCHAR(64),
  payload JSONB NOT NULL DEFAULT '{}'::jsonb,
  ip_address INET,
  created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_admin_audit_logs_created_at ON admin_audit_logs(created_at DESC);

INSERT INTO maps(id, code, name, width, height, default_spawn_x, default_spawn_y, tick_ms)
VALUES
  (1, 'default', 'Default Map', 2048, 2048, 100, 100, 50)
ON CONFLICT (id) DO NOTHING;

INSERT INTO admin_roles(code, name)
VALUES
  ('superadmin', 'Super Admin'),
  ('admin', 'Admin'),
  ('gm', 'Game Master'),
  ('support', 'Support'),
  ('readonly', 'Read Only')
ON CONFLICT (code) DO NOTHING;

INSERT INTO admin_permissions(code, description)
VALUES
  ('accounts.read', 'Read accounts'),
  ('accounts.write', 'Modify accounts'),
  ('accounts.ban', 'Ban or unban accounts'),
  ('characters.read', 'Read characters'),
  ('characters.write', 'Modify characters'),
  ('characters.disconnect', 'Disconnect active character sessions'),
  ('world.read', 'Read world state'),
  ('world.write', 'Modify world state'),
  ('moderation.write', 'Mute/Jail/Ban'),
  ('broadcast.send', 'Send broadcast'),
  ('audit.read', 'Read admin audit trail'),
  ('metrics.read', 'Read observability metrics')
ON CONFLICT (code) DO NOTHING;

WITH role_map AS (
  SELECT id, code FROM admin_roles
), perm_map AS (
  SELECT id, code FROM admin_permissions
)
INSERT INTO admin_role_permissions(role_id, permission_id)
SELECT r.id, p.id
FROM role_map r
JOIN perm_map p ON (
     r.code = 'superadmin'
  OR (r.code = 'admin' AND p.code IN ('accounts.read','accounts.write','accounts.ban','characters.read','characters.write','world.read','world.write','broadcast.send','audit.read','metrics.read'))
  OR (r.code = 'gm' AND p.code IN ('characters.read','characters.write','characters.disconnect','world.read','world.write','moderation.write','broadcast.send','metrics.read'))
  OR (r.code = 'support' AND p.code IN ('accounts.read','characters.read','moderation.write'))
  OR (r.code = 'readonly' AND p.code IN ('accounts.read','characters.read','world.read','audit.read','metrics.read'))
)
ON CONFLICT DO NOTHING;
