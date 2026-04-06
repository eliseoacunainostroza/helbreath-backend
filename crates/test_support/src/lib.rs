use uuid::Uuid;

pub fn unique_username(prefix: &str) -> String {
    format!("{}-{}", prefix, Uuid::new_v4().simple())
}

pub fn test_db_url() -> String {
    std::env::var("HB_TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://hb:hbpass@127.0.0.1:5432/helbreath_test".to_string())
}
