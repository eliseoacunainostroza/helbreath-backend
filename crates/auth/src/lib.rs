use anyhow::{anyhow, Result};
use argon2::{password_hash::PasswordHash, Argon2, PasswordVerifier};
use chrono::Utc;
use domain::{AccountStatus, SessionId};
use infrastructure::{default_session_expiry, AccountRepository};
use observability::{MetricsRegistry, METRICS};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct LoginOutcome {
    pub accepted: bool,
    pub account_id: Option<Uuid>,
    pub reason: Option<String>,
}

pub struct AuthService<R>
where
    R: AccountRepository,
{
    repo: R,
    session_ttl_seconds: u64,
    gateway_node: String,
}

impl<R> AuthService<R>
where
    R: AccountRepository,
{
    pub fn new(repo: R, session_ttl_seconds: u64, gateway_node: impl Into<String>) -> Self {
        Self {
            repo,
            session_ttl_seconds,
            gateway_node: gateway_node.into(),
        }
    }

    pub async fn validate_credentials(
        &self,
        session_id: SessionId,
        username: &str,
        password: &str,
        remote_ip: &str,
    ) -> Result<LoginOutcome> {
        let maybe_account = self.repo.find_account_for_login(username).await?;
        let Some(account) = maybe_account else {
            MetricsRegistry::inc(&METRICS.auth_failures_total);
            return Ok(LoginOutcome {
                accepted: false,
                account_id: None,
                reason: Some("invalid_credentials".to_string()),
            });
        };

        if account.status != AccountStatus::Active {
            MetricsRegistry::inc(&METRICS.auth_failures_total);
            return Ok(LoginOutcome {
                accepted: false,
                account_id: Some(account.id),
                reason: Some("account_not_active".to_string()),
            });
        }

        let password_ok = if let Some(plain) = account.password_hash.strip_prefix("plain:") {
            // Test-only escape hatch for smoke fixtures. Production accounts must use Argon2 hashes.
            password == plain
        } else {
            let parsed_hash = PasswordHash::new(&account.password_hash).map_err(|e| {
                anyhow!(
                    "invalid stored password hash for user {}: {}",
                    account.username,
                    e
                )
            })?;
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed_hash)
                .is_ok()
        };

        if !password_ok {
            MetricsRegistry::inc(&METRICS.auth_failures_total);
            return Ok(LoginOutcome {
                accepted: false,
                account_id: Some(account.id),
                reason: Some("invalid_credentials".to_string()),
            });
        }

        self.repo
            .upsert_session(
                session_id,
                account.id,
                &self.gateway_node,
                remote_ip,
                default_session_expiry(self.session_ttl_seconds),
            )
            .await?;

        MetricsRegistry::inc(&METRICS.logins_total);

        tracing::info!(
            session_id = %session_id,
            account_id = %account.id,
            username = %account.username,
            at = %Utc::now(),
            "login accepted"
        );

        Ok(LoginOutcome {
            accepted: true,
            account_id: Some(account.id),
            reason: None,
        })
    }

    pub async fn close_session(&self, session_id: SessionId) -> Result<()> {
        self.repo.close_session(session_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use chrono::Utc;
    use infrastructure::{AccountAuthRecord, NewCharacterParams};
    use std::collections::HashMap;
    use std::sync::Arc;
    use tokio::sync::Mutex;

    #[derive(Clone, Default)]
    struct InMemoryRepo {
        accounts: Arc<Mutex<HashMap<String, AccountAuthRecord>>>,
        closed: Arc<Mutex<Vec<SessionId>>>,
    }

    #[async_trait]
    impl AccountRepository for InMemoryRepo {
        async fn find_account_for_login(
            &self,
            username: &str,
        ) -> Result<Option<AccountAuthRecord>> {
            Ok(self.accounts.lock().await.get(username).cloned())
        }

        async fn upsert_session(
            &self,
            _session_id: SessionId,
            _account_id: domain::AccountId,
            _gateway_node: &str,
            _remote_ip: &str,
            _expires_at: chrono::DateTime<Utc>,
        ) -> Result<()> {
            Ok(())
        }

        async fn close_session(&self, session_id: SessionId) -> Result<()> {
            self.closed.lock().await.push(session_id);
            Ok(())
        }

        async fn get_session_account(
            &self,
            _session_id: SessionId,
        ) -> Result<Option<domain::AccountId>> {
            Ok(None)
        }

        async fn list_characters_for_account(
            &self,
            _account_id: domain::AccountId,
        ) -> Result<Vec<domain::Character>> {
            Ok(Vec::new())
        }

        async fn create_character(
            &self,
            _account_id: domain::AccountId,
            _params: NewCharacterParams,
        ) -> Result<domain::Character> {
            anyhow::bail!("not implemented for test")
        }

        async fn delete_character(
            &self,
            _account_id: domain::AccountId,
            _character_id: domain::CharacterId,
        ) -> Result<bool> {
            Ok(false)
        }

        async fn load_character(
            &self,
            _account_id: domain::AccountId,
            _character_id: domain::CharacterId,
        ) -> Result<Option<domain::Character>> {
            Ok(None)
        }

        async fn bind_session_character(
            &self,
            _session_id: SessionId,
            _account_id: domain::AccountId,
            _character_id: domain::CharacterId,
        ) -> Result<Option<domain::Character>> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn plain_password_fixture_login_is_supported() {
        let repo = InMemoryRepo::default();
        repo.accounts.lock().await.insert(
            "fixture".to_string(),
            AccountAuthRecord {
                id: Uuid::new_v4(),
                username: "fixture".to_string(),
                password_hash: "plain:testpass".to_string(),
                status: AccountStatus::Active,
            },
        );

        let svc = AuthService::new(repo, 3600, "test-gw");
        let out = svc
            .validate_credentials(Uuid::new_v4(), "fixture", "testpass", "127.0.0.1")
            .await
            .expect("validate credentials");
        assert!(out.accepted);
    }
}
