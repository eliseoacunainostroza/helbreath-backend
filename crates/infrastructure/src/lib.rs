use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use domain::{
    Account, AccountId, AccountStatus, AdminRole, Character, CharacterClass, CharacterId, SessionId,
};
use observability::{MetricsRegistry, METRICS};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use uuid::Uuid;

#[derive(Clone)]
pub struct PgRepository {
    pool: PgPool,
}

impl PgRepository {
    pub async fn new(settings: &config::Settings) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(settings.database_max_conn)
            .connect(&settings.database_url)
            .await
            .with_context(|| "failed to connect to postgres")?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn readiness_check(&self) -> Result<()> {
        self.timed_query(sqlx::query("SELECT 1").execute(&self.pool))
            .await
            .map(|_| ())
    }

    pub async fn persist_position_snapshot(
        &self,
        character_id: CharacterId,
        map_id: i32,
        x: i32,
        y: i32,
    ) -> Result<()> {
        let started = std::time::Instant::now();
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE characters
            SET map_id = $2,
                pos_x = $3,
                pos_y = $4,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(character_id)
        .bind(map_id)
        .bind(x)
        .bind(y)
        .execute(&mut *tx)
        .await?;

        let payload = serde_json::json!({
            "x": x,
            "y": y,
            "source": "map_tick"
        });
        sqlx::query(
            r#"
            INSERT INTO game_events(event_type, actor_character_id, map_id, payload)
            VALUES ('character.position', $1, $2, $3)
            "#,
        )
        .bind(character_id)
        .bind(map_id)
        .bind(payload)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        MetricsRegistry::set(
            &METRICS.db_latency_ms_last,
            started.elapsed().as_millis() as u64,
        );
        Ok(())
    }

    pub async fn persist_inventory_event(
        &self,
        character_id: CharacterId,
        map_id: i32,
        reason: &str,
    ) -> Result<()> {
        let started = std::time::Instant::now();
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            r#"
            UPDATE inventories
            SET version = version + 1,
                updated_at = now()
            WHERE character_id = $1
            "#,
        )
        .bind(character_id)
        .execute(&mut *tx)
        .await?;

        let payload = serde_json::json!({
            "reason": reason,
            "source": "map_tick"
        });
        sqlx::query(
            r#"
            INSERT INTO game_events(event_type, actor_character_id, map_id, payload)
            VALUES ('inventory.change', $1, $2, $3)
            "#,
        )
        .bind(character_id)
        .bind(map_id)
        .bind(payload)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        MetricsRegistry::set(
            &METRICS.db_latency_ms_last,
            started.elapsed().as_millis() as u64,
        );
        Ok(())
    }

    pub async fn persist_combat_event(
        &self,
        attacker_character_id: CharacterId,
        defender_character_id: CharacterId,
        map_id: i32,
        damage: i32,
    ) -> Result<()> {
        let started = std::time::Instant::now();
        let mut tx = self.pool.begin().await?;

        let payload = serde_json::json!({
            "source": "map_tick"
        });
        sqlx::query(
            r#"
            INSERT INTO combat_logs(
                attacker_entity_id,
                defender_entity_id,
                damage,
                payload
            )
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(attacker_character_id)
        .bind(defender_character_id)
        .bind(damage)
        .bind(payload)
        .execute(&mut *tx)
        .await?;

        let game_event_payload = serde_json::json!({
            "defender_character_id": defender_character_id,
            "damage": damage
        });
        sqlx::query(
            r#"
            INSERT INTO game_events(event_type, actor_character_id, map_id, payload)
            VALUES ('combat.hit', $1, $2, $3)
            "#,
        )
        .bind(attacker_character_id)
        .bind(map_id)
        .bind(game_event_payload)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        MetricsRegistry::set(
            &METRICS.db_latency_ms_last,
            started.elapsed().as_millis() as u64,
        );
        Ok(())
    }

    async fn timed_query<T, F>(&self, fut: F) -> Result<T>
    where
        F: std::future::Future<Output = Result<T, sqlx::Error>>,
    {
        let started = std::time::Instant::now();
        let result = fut.await;
        MetricsRegistry::set(
            &METRICS.db_latency_ms_last,
            started.elapsed().as_millis() as u64,
        );
        result.map_err(Into::into)
    }
}

pub fn build_redis_client(settings: &config::Settings) -> Result<Option<redis::Client>> {
    if !settings.redis_enabled {
        return Ok(None);
    }
    Ok(Some(redis::Client::open(settings.redis_url.clone())?))
}

#[derive(Debug, Clone)]
pub struct AccountAuthRecord {
    pub id: AccountId,
    pub username: String,
    pub password_hash: String,
    pub status: AccountStatus,
}

#[derive(Debug, Clone)]
pub struct NewCharacterParams {
    pub name: String,
    pub class: CharacterClass,
    pub gender: i16,
    pub skin_color: i16,
    pub hair_style: i16,
    pub hair_color: i16,
    pub underwear_color: i16,
    pub stats: [i16; 6],
}

#[derive(Debug, Clone)]
pub struct AdminAuthRecord {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub roles: Vec<AdminRole>,
}

#[derive(Debug, Clone)]
pub struct AdminAuditInsert {
    pub admin_user_id: Uuid,
    pub action_type: String,
    pub target_type: String,
    pub target_id: String,
    pub payload: serde_json::Value,
    pub request_id: Option<String>,
    pub ip_address: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AdminAuditRecord {
    pub id: Uuid,
    pub admin_user_id: Option<Uuid>,
    pub action_type: String,
    pub target_type: String,
    pub target_id: String,
    pub request_id: Option<String>,
    pub payload: serde_json::Value,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MapRuntimeInfo {
    pub id: i32,
    pub code: String,
    pub name: String,
    pub tick_ms: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InventoryRecord {
    pub slot: i32,
    pub item_id: i64,
    pub quantity: i32,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SanctionRecord {
    pub id: Uuid,
    pub account_id: Option<Uuid>,
    pub character_id: Option<Uuid>,
    pub sanction_type: String,
    pub reason: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CreateSanctionInput {
    pub account_id: Option<AccountId>,
    pub character_id: Option<CharacterId>,
    pub sanction_type: String,
    pub reason: String,
    pub starts_at: DateTime<Utc>,
    pub ends_at: Option<DateTime<Utc>>,
    pub issued_by_admin_id: Option<Uuid>,
}

#[async_trait]
pub trait AccountRepository: Send + Sync {
    async fn find_account_for_login(&self, username: &str) -> Result<Option<AccountAuthRecord>>;
    async fn upsert_session(
        &self,
        session_id: SessionId,
        account_id: AccountId,
        gateway_node: &str,
        remote_ip: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<()>;
    async fn close_session(&self, session_id: SessionId) -> Result<()>;
    async fn get_session_account(&self, session_id: SessionId) -> Result<Option<AccountId>>;
    async fn list_characters_for_account(&self, account_id: AccountId) -> Result<Vec<Character>>;
    async fn create_character(
        &self,
        account_id: AccountId,
        params: NewCharacterParams,
    ) -> Result<Character>;
    async fn delete_character(
        &self,
        account_id: AccountId,
        character_id: CharacterId,
    ) -> Result<bool>;
    async fn load_character(
        &self,
        account_id: AccountId,
        character_id: CharacterId,
    ) -> Result<Option<Character>>;
    async fn bind_session_character(
        &self,
        session_id: SessionId,
        account_id: AccountId,
        character_id: CharacterId,
    ) -> Result<Option<Character>>;
}

#[async_trait]
pub trait AdminRepository: Send + Sync {
    async fn find_admin_for_login(&self, email: &str) -> Result<Option<AdminAuthRecord>>;
    async fn insert_admin_audit(&self, record: AdminAuditInsert) -> Result<()>;
    async fn list_admin_audit(&self, limit: i64) -> Result<Vec<AdminAuditRecord>>;

    async fn search_accounts(&self, query: &str, limit: i64) -> Result<Vec<Account>>;
    async fn set_account_status(&self, account_id: AccountId, status: AccountStatus) -> Result<()>;

    async fn search_characters(&self, query: &str, limit: i64) -> Result<Vec<Character>>;
    async fn move_character(
        &self,
        character_id: CharacterId,
        map_id: i32,
        x: i32,
        y: i32,
    ) -> Result<()>;
    async fn get_character_inventory(
        &self,
        character_id: CharacterId,
    ) -> Result<Vec<InventoryRecord>>;

    async fn list_maps(&self) -> Result<Vec<MapRuntimeInfo>>;
    async fn list_active_map_ids(&self) -> Result<Vec<i32>>;
    async fn activate_map(&self, map_id: i32) -> Result<bool>;

    async fn create_sanction(&self, input: CreateSanctionInput) -> Result<()>;

    async fn list_account_sanctions(
        &self,
        account_id: AccountId,
        limit: i64,
    ) -> Result<Vec<SanctionRecord>>;
}

#[async_trait]
impl AccountRepository for PgRepository {
    async fn find_account_for_login(&self, username: &str) -> Result<Option<AccountAuthRecord>> {
        let row = self
            .timed_query(
                sqlx::query(
                    r#"
                SELECT id, username, password_hash, status
                FROM accounts
                WHERE username = $1
                LIMIT 1
                "#,
                )
                .bind(username)
                .fetch_optional(&self.pool),
            )
            .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let status = match row.get::<String, _>("status").as_str() {
            "active" => AccountStatus::Active,
            "suspended" => AccountStatus::Suspended,
            _ => AccountStatus::Banned,
        };

        Ok(Some(AccountAuthRecord {
            id: row.get("id"),
            username: row.get("username"),
            password_hash: row.get("password_hash"),
            status,
        }))
    }

    async fn upsert_session(
        &self,
        session_id: SessionId,
        account_id: AccountId,
        gateway_node: &str,
        remote_ip: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<()> {
        self
            .timed_query(
                sqlx::query(
                    r#"
                INSERT INTO sessions(id, account_id, gateway_node, remote_ip, state, issued_at, expires_at)
                VALUES ($1, $2, $3, $4::inet, 'authenticated', now(), $5)
                ON CONFLICT (id) DO UPDATE
                SET account_id = EXCLUDED.account_id,
                    gateway_node = EXCLUDED.gateway_node,
                    remote_ip = EXCLUDED.remote_ip,
                    state = EXCLUDED.state,
                    expires_at = EXCLUDED.expires_at,
                    closed_at = NULL
                "#,
                )
                .bind(session_id)
                .bind(account_id)
                .bind(gateway_node)
                .bind(remote_ip)
                .bind(expires_at)
                .execute(&self.pool),
            )
            .await?;

        Ok(())
    }

    async fn close_session(&self, session_id: SessionId) -> Result<()> {
        self.timed_query(
            sqlx::query("UPDATE sessions SET state='closed', closed_at=now() WHERE id=$1")
                .bind(session_id)
                .execute(&self.pool),
        )
        .await?;

        Ok(())
    }

    async fn get_session_account(&self, session_id: SessionId) -> Result<Option<AccountId>> {
        let row = self
            .timed_query(
                sqlx::query(
                    r#"
                SELECT account_id
                FROM sessions
                WHERE id=$1 AND state != 'closed'
                LIMIT 1
                "#,
                )
                .bind(session_id)
                .fetch_optional(&self.pool),
            )
            .await?;

        Ok(row.and_then(|r| r.get::<Option<AccountId>, _>("account_id")))
    }

    async fn list_characters_for_account(&self, account_id: AccountId) -> Result<Vec<Character>> {
        let rows = self
            .timed_query(
                sqlx::query(
                    r#"
                SELECT id, account_id, name, class_code, map_id, pos_x, pos_y, level, exp, hp, mp, sp,
                       str_stat, vit_stat, dex_stat, int_stat, mag_stat, chr_stat,
                       created_at, updated_at
                FROM characters
                WHERE account_id=$1 AND is_deleted=false
                ORDER BY slot ASC
                "#,
                )
                .bind(account_id)
                .fetch_all(&self.pool),
            )
            .await?;

        Ok(rows.into_iter().map(row_to_character).collect())
    }

    async fn create_character(
        &self,
        account_id: AccountId,
        params: NewCharacterParams,
    ) -> Result<Character> {
        let class_code = character_class_code(params.class);
        let rows = self
            .timed_query(
                sqlx::query(
                    r#"
                WITH slot_pick AS (
                    SELECT s AS slot
                    FROM generate_series(0, 3) AS s
                    WHERE NOT EXISTS (
                        SELECT 1
                        FROM characters c
                        WHERE c.account_id = $1
                          AND c.slot = s
                          AND c.is_deleted = false
                    )
                    ORDER BY s
                    LIMIT 1
                )
                INSERT INTO characters(
                    id, account_id, name, slot, class_code, gender, skin_color, hair_style, hair_color, underwear_color,
                    map_id, pos_x, pos_y, level, exp, hp, mp, sp,
                    str_stat, vit_stat, dex_stat, int_stat, mag_stat, chr_stat,
                    is_deleted
                )
                SELECT
                    gen_random_uuid(), $1, $2, slot_pick.slot, $3, $4, $5, $6, $7, $8,
                    m.id, m.default_spawn_x, m.default_spawn_y, 1, 0, 100, 50, 50,
                    $9, $10, $11, $12, $13, $14,
                    false
                FROM slot_pick
                JOIN maps m ON m.id = 1
                RETURNING id, account_id, name, class_code, map_id, pos_x, pos_y, level, exp, hp, mp, sp,
                          str_stat, vit_stat, dex_stat, int_stat, mag_stat, chr_stat, created_at, updated_at
                "#,
                )
                .bind(account_id)
                .bind(params.name)
                .bind(class_code)
                .bind(params.gender)
                .bind(params.skin_color)
                .bind(params.hair_style)
                .bind(params.hair_color)
                .bind(params.underwear_color)
                .bind(params.stats[0])
                .bind(params.stats[1])
                .bind(params.stats[2])
                .bind(params.stats[3])
                .bind(params.stats[4])
                .bind(params.stats[5])
                .fetch_all(&self.pool),
            )
            .await?;

        let Some(row) = rows.into_iter().next() else {
            anyhow::bail!("character slot limit reached for account");
        };
        Ok(row_to_character(row))
    }

    async fn delete_character(
        &self,
        account_id: AccountId,
        character_id: CharacterId,
    ) -> Result<bool> {
        let rows = self
            .timed_query(
                sqlx::query(
                    r#"
                UPDATE characters
                SET is_deleted=true, updated_at=now()
                WHERE id=$1 AND account_id=$2 AND is_deleted=false
                "#,
                )
                .bind(character_id)
                .bind(account_id)
                .execute(&self.pool),
            )
            .await?
            .rows_affected();

        if rows > 0 {
            let _ = self
                .timed_query(
                    sqlx::query(
                        "UPDATE sessions SET character_id=NULL WHERE account_id=$1 AND character_id=$2 AND state!='closed'",
                    )
                    .bind(account_id)
                    .bind(character_id)
                    .execute(&self.pool),
                )
                .await;
        }

        Ok(rows > 0)
    }

    async fn load_character(
        &self,
        account_id: AccountId,
        character_id: CharacterId,
    ) -> Result<Option<Character>> {
        let row = self
            .timed_query(
                sqlx::query(
                    r#"
                SELECT id, account_id, name, class_code, map_id, pos_x, pos_y, level, exp, hp, mp, sp,
                       str_stat, vit_stat, dex_stat, int_stat, mag_stat, chr_stat,
                       created_at, updated_at
                FROM characters
                WHERE account_id=$1 AND id=$2 AND is_deleted=false
                LIMIT 1
                "#,
                )
                .bind(account_id)
                .bind(character_id)
                .fetch_optional(&self.pool),
            )
            .await?;

        Ok(row.map(row_to_character))
    }

    async fn bind_session_character(
        &self,
        session_id: SessionId,
        account_id: AccountId,
        character_id: CharacterId,
    ) -> Result<Option<Character>> {
        let Some(character) = self.load_character(account_id, character_id).await? else {
            return Ok(None);
        };

        let updated = self
            .timed_query(
                sqlx::query(
                    r#"
                UPDATE sessions
                SET character_id=$3,
                    state='character_selected',
                    last_seen_at=now()
                WHERE id=$1 AND account_id=$2 AND state != 'closed'
                "#,
                )
                .bind(session_id)
                .bind(account_id)
                .bind(character_id)
                .execute(&self.pool),
            )
            .await?
            .rows_affected();

        if updated == 0 {
            return Ok(None);
        }

        Ok(Some(character))
    }
}

#[async_trait]
impl AdminRepository for PgRepository {
    async fn find_admin_for_login(&self, email: &str) -> Result<Option<AdminAuthRecord>> {
        let row = self
            .timed_query(
                sqlx::query(
                    "SELECT id, email, password_hash FROM admin_users WHERE email=$1 AND status='active' LIMIT 1",
                )
                .bind(email)
                .fetch_optional(&self.pool),
            )
            .await?;

        let Some(row) = row else {
            return Ok(None);
        };

        let admin_id: Uuid = row.get("id");

        let role_rows = self
            .timed_query(
                sqlx::query(
                    r#"
                SELECT r.code
                FROM admin_user_roles ur
                JOIN admin_roles r ON r.id = ur.role_id
                WHERE ur.admin_user_id = $1
                "#,
                )
                .bind(admin_id)
                .fetch_all(&self.pool),
            )
            .await?;

        let roles = role_rows
            .into_iter()
            .map(
                |role_row| match role_row.get::<String, _>("code").as_str() {
                    "superadmin" => AdminRole::SuperAdmin,
                    "admin" => AdminRole::Admin,
                    "gm" => AdminRole::Gm,
                    "support" => AdminRole::Support,
                    _ => AdminRole::ReadOnly,
                },
            )
            .collect();

        Ok(Some(AdminAuthRecord {
            id: admin_id,
            email: row.get("email"),
            password_hash: row.get("password_hash"),
            roles,
        }))
    }

    async fn insert_admin_audit(&self, record: AdminAuditInsert) -> Result<()> {
        self
            .timed_query(
                sqlx::query(
                    r#"
                INSERT INTO admin_audit_logs(admin_user_id, action_type, target_type, target_id, request_id, payload, ip_address)
                    VALUES ($1,$2,$3,$4,$5,$6,$7::inet)
                "#,
                )
                .bind(record.admin_user_id)
                .bind(record.action_type)
                .bind(record.target_type)
                .bind(record.target_id)
                .bind(record.request_id)
                .bind(record.payload)
                .bind(record.ip_address)
                .execute(&self.pool),
            )
            .await?;

        MetricsRegistry::inc(&METRICS.admin_actions_total);
        Ok(())
    }

    async fn list_admin_audit(&self, limit: i64) -> Result<Vec<AdminAuditRecord>> {
        let rows = self
            .timed_query(
                sqlx::query(
                    r#"
                    SELECT id, admin_user_id, action_type, target_type, target_id, request_id, payload, ip_address::text AS ip_address, created_at
                    FROM admin_audit_logs
                    ORDER BY created_at DESC
                    LIMIT $1
                    "#,
                )
                .bind(limit)
                .fetch_all(&self.pool),
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| AdminAuditRecord {
                id: row.get("id"),
                admin_user_id: row.get("admin_user_id"),
                action_type: row.get("action_type"),
                target_type: row.get("target_type"),
                target_id: row.get("target_id"),
                request_id: row.get("request_id"),
                payload: row.get("payload"),
                ip_address: row.get("ip_address"),
                created_at: row.get("created_at"),
            })
            .collect())
    }

    async fn search_accounts(&self, query: &str, limit: i64) -> Result<Vec<Account>> {
        let pattern = format!("%{}%", query);
        let rows = self
            .timed_query(
                sqlx::query(
                    r#"
                SELECT id, username, email, status, created_at, updated_at
                FROM accounts
                WHERE username ILIKE $1 OR COALESCE(email,'') ILIKE $1
                ORDER BY username ASC
                LIMIT $2
                "#,
                )
                .bind(pattern)
                .bind(limit)
                .fetch_all(&self.pool),
            )
            .await?;

        let out = rows
            .into_iter()
            .map(|row| Account {
                id: row.get("id"),
                username: row.get("username"),
                email: row.get("email"),
                status: match row.get::<String, _>("status").as_str() {
                    "active" => AccountStatus::Active,
                    "suspended" => AccountStatus::Suspended,
                    _ => AccountStatus::Banned,
                },
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            })
            .collect();

        Ok(out)
    }

    async fn set_account_status(&self, account_id: AccountId, status: AccountStatus) -> Result<()> {
        let status_code = match status {
            AccountStatus::Active => "active",
            AccountStatus::Suspended => "suspended",
            AccountStatus::Banned => "banned",
        };

        self.timed_query(
            sqlx::query("UPDATE accounts SET status=$1, updated_at=now() WHERE id=$2")
                .bind(status_code)
                .bind(account_id)
                .execute(&self.pool),
        )
        .await?;

        Ok(())
    }

    async fn search_characters(&self, query: &str, limit: i64) -> Result<Vec<Character>> {
        let pattern = format!("%{}%", query);
        let rows = self
            .timed_query(
                sqlx::query(
                    r#"
                SELECT id, account_id, name, class_code, map_id, pos_x, pos_y, level, exp, hp, mp, sp,
                       str_stat, vit_stat, dex_stat, int_stat, mag_stat, chr_stat,
                       created_at, updated_at
                FROM characters
                WHERE is_deleted=false AND name ILIKE $1
                ORDER BY name ASC
                LIMIT $2
                "#,
                )
                .bind(pattern)
                .bind(limit)
                .fetch_all(&self.pool),
            )
            .await?;

        Ok(rows.into_iter().map(row_to_character).collect())
    }

    async fn move_character(
        &self,
        character_id: CharacterId,
        map_id: i32,
        x: i32,
        y: i32,
    ) -> Result<()> {
        self.timed_query(
            sqlx::query(
                "UPDATE characters SET map_id=$1, pos_x=$2, pos_y=$3, updated_at=now() WHERE id=$4",
            )
            .bind(map_id)
            .bind(x)
            .bind(y)
            .bind(character_id)
            .execute(&self.pool),
        )
        .await?;

        Ok(())
    }

    async fn get_character_inventory(
        &self,
        character_id: CharacterId,
    ) -> Result<Vec<InventoryRecord>> {
        let rows = self
            .timed_query(
                sqlx::query(
                    r#"
                SELECT ii.slot, ii.item_id, ii.quantity, ii.metadata
                FROM inventories i
                JOIN inventory_items ii ON ii.inventory_id = i.id
                WHERE i.character_id=$1
                ORDER BY ii.slot ASC
                "#,
                )
                .bind(character_id)
                .fetch_all(&self.pool),
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| InventoryRecord {
                slot: row.get("slot"),
                item_id: row.get("item_id"),
                quantity: row.get("quantity"),
                metadata: row.get("metadata"),
            })
            .collect())
    }

    async fn list_maps(&self) -> Result<Vec<MapRuntimeInfo>> {
        let rows = self
            .timed_query(
                sqlx::query("SELECT id, code, name, tick_ms FROM maps ORDER BY id")
                    .fetch_all(&self.pool),
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| MapRuntimeInfo {
                id: row.get("id"),
                code: row.get("code"),
                name: row.get("name"),
                tick_ms: row.get("tick_ms"),
            })
            .collect())
    }

    async fn list_active_map_ids(&self) -> Result<Vec<i32>> {
        let rows = self
            .timed_query(
                sqlx::query(
                    r#"
                    SELECT DISTINCT map_id
                    FROM map_instances
                    WHERE status='active'
                      AND stopped_at IS NULL
                    ORDER BY map_id
                    "#,
                )
                .fetch_all(&self.pool),
            )
            .await?;

        Ok(rows.into_iter().map(|row| row.get("map_id")).collect())
    }

    async fn activate_map(&self, map_id: i32) -> Result<bool> {
        let map_exists = self
            .timed_query(
                sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM maps WHERE id=$1)")
                    .bind(map_id)
                    .fetch_one(&self.pool),
            )
            .await?;

        if !map_exists {
            return Ok(false);
        }

        self.timed_query(
            sqlx::query(
                r#"
                    INSERT INTO map_instances(map_id, shard_code, status, started_at, stopped_at)
                    VALUES ($1, 'default', 'active', now(), NULL)
                    ON CONFLICT (map_id, shard_code)
                    DO UPDATE
                    SET status='active',
                        started_at = CASE
                            WHEN map_instances.status='active' THEN map_instances.started_at
                            ELSE now()
                        END,
                        stopped_at = NULL
                    "#,
            )
            .bind(map_id)
            .execute(&self.pool),
        )
        .await?;

        Ok(true)
    }

    async fn create_sanction(&self, input: CreateSanctionInput) -> Result<()> {
        self
            .timed_query(
                sqlx::query(
                    r#"
                    INSERT INTO sanctions(account_id, character_id, sanction_type, reason, starts_at, ends_at, issued_by_admin_id)
                    VALUES ($1,$2,$3,$4,$5,$6,$7)
                    "#,
                )
                .bind(input.account_id)
                .bind(input.character_id)
                .bind(input.sanction_type)
                .bind(input.reason)
                .bind(input.starts_at)
                .bind(input.ends_at)
                .bind(input.issued_by_admin_id)
                .execute(&self.pool),
            )
            .await?;

        Ok(())
    }

    async fn list_account_sanctions(
        &self,
        account_id: AccountId,
        limit: i64,
    ) -> Result<Vec<SanctionRecord>> {
        let rows = self
            .timed_query(
                sqlx::query(
                    r#"
                    SELECT id, account_id, character_id, sanction_type, reason, starts_at, ends_at, created_at
                    FROM sanctions
                    WHERE account_id = $1
                    ORDER BY created_at DESC
                    LIMIT $2
                    "#,
                )
                .bind(account_id)
                .bind(limit)
                .fetch_all(&self.pool),
            )
            .await?;

        Ok(rows
            .into_iter()
            .map(|row| SanctionRecord {
                id: row.get("id"),
                account_id: row.get("account_id"),
                character_id: row.get("character_id"),
                sanction_type: row.get("sanction_type"),
                reason: row.get("reason"),
                starts_at: row.get("starts_at"),
                ends_at: row.get("ends_at"),
                created_at: row.get("created_at"),
            })
            .collect())
    }
}

fn row_to_character(row: sqlx::postgres::PgRow) -> Character {
    Character {
        id: row.get("id"),
        account_id: row.get("account_id"),
        name: row.get("name"),
        class: parse_character_class(row.get::<String, _>("class_code").as_str()),
        map_id: row.get("map_id"),
        x: row.get("pos_x"),
        y: row.get("pos_y"),
        level: row.get("level"),
        exp: row.get("exp"),
        hp: row.get("hp"),
        mp: row.get("mp"),
        sp: row.get("sp"),
        stats: domain::CharacterStats {
            strength: row.get("str_stat"),
            vitality: row.get("vit_stat"),
            dexterity: row.get("dex_stat"),
            intelligence: row.get("int_stat"),
            magic: row.get("mag_stat"),
            charisma: row.get("chr_stat"),
        },
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn parse_character_class(raw: &str) -> CharacterClass {
    match raw {
        "mage" => CharacterClass::Mage,
        "archer" => CharacterClass::Archer,
        _ => CharacterClass::Warrior,
    }
}

fn character_class_code(class: CharacterClass) -> &'static str {
    match class {
        CharacterClass::Warrior => "warrior",
        CharacterClass::Mage => "mage",
        CharacterClass::Archer => "archer",
    }
}

pub fn default_session_expiry(seconds: u64) -> DateTime<Utc> {
    Utc::now() + Duration::seconds(seconds as i64)
}
