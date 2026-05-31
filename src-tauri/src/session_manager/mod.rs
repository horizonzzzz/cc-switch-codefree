pub mod providers;
pub mod terminal;

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use providers::{claude, codefree_o, codex, gemini, hermes, openclaw, opencode};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub provider_id: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_active_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMessage {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ts: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSessionRequest {
    pub provider_id: String,
    pub session_id: String,
    pub source_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteSessionOutcome {
    pub provider_id: String,
    pub session_id: String,
    pub source_path: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub fn scan_sessions() -> Vec<SessionMeta> {
    let (r1, r2, r3, r4, r5, r6, r7) = std::thread::scope(|s| {
        let h1 = s.spawn(codex::scan_sessions);
        let h2 = s.spawn(claude::scan_sessions);
        let h3 = s.spawn(opencode::scan_sessions);
        let h4 = s.spawn(openclaw::scan_sessions);
        let h5 = s.spawn(gemini::scan_sessions);
        let h6 = s.spawn(hermes::scan_sessions);
        let h7 = s.spawn(codefree_o::scan_sessions);
        (
            h1.join().unwrap_or_default(),
            h2.join().unwrap_or_default(),
            h3.join().unwrap_or_default(),
            h4.join().unwrap_or_default(),
            h5.join().unwrap_or_default(),
            h6.join().unwrap_or_default(),
            h7.join().unwrap_or_default(),
        )
    });

    let mut sessions = Vec::new();
    sessions.extend(r1);
    sessions.extend(r2);
    sessions.extend(r3);
    sessions.extend(r4);
    sessions.extend(r5);
    sessions.extend(r6);
    sessions.extend(r7);

    sessions.sort_by(|a, b| {
        let a_ts = a.last_active_at.or(a.created_at).unwrap_or(0);
        let b_ts = b.last_active_at.or(b.created_at).unwrap_or(0);
        b_ts.cmp(&a_ts)
    });

    sessions
}

pub fn load_messages(provider_id: &str, source_path: &str) -> Result<Vec<SessionMessage>, String> {
    // SQLite sessions use a "sqlite:" prefixed source_path
    if provider_id == "opencode" && source_path.starts_with("sqlite:") {
        return opencode::load_messages_sqlite(source_path);
    }
    if provider_id == "codefree-o" && source_path.starts_with("sqlite:") {
        return codefree_o::load_messages_sqlite(source_path);
    }
    if provider_id == "hermes" && source_path.starts_with("sqlite:") {
        return hermes::load_messages_sqlite(source_path);
    }

    let path = Path::new(source_path);
    match provider_id {
        "codex" => codex::load_messages(path),
        "claude" => claude::load_messages(path),
        "opencode" => opencode::load_messages(path),
        "codefree-o" => Err("codefree-o sessions are SQLite-backed only".to_string()),
        "openclaw" => openclaw::load_messages(path),
        "gemini" => gemini::load_messages(path),
        "hermes" => hermes::load_messages(path),
        _ => Err(format!("Unsupported provider: {provider_id}")),
    }
}

pub fn delete_session(
    provider_id: &str,
    session_id: &str,
    source_path: &str,
) -> Result<bool, String> {
    // SQLite sessions bypass the file-based deletion path
    if provider_id == "opencode" && source_path.starts_with("sqlite:") {
        return opencode::delete_session_sqlite(session_id, source_path);
    }
    if provider_id == "codefree-o" && source_path.starts_with("sqlite:") {
        return codefree_o::delete_session_sqlite(session_id, source_path);
    }
    if provider_id == "hermes" && source_path.starts_with("sqlite:") {
        return hermes::delete_session_sqlite(session_id, source_path);
    }

    let roots = provider_roots(provider_id)?;
    delete_session_with_roots(provider_id, session_id, Path::new(source_path), &roots)
}

pub fn delete_sessions(requests: &[DeleteSessionRequest]) -> Vec<DeleteSessionOutcome> {
    collect_delete_session_outcomes(requests, |request| {
        delete_session(
            &request.provider_id,
            &request.session_id,
            &request.source_path,
        )
    })
}

fn delete_session_with_roots(
    provider_id: &str,
    session_id: &str,
    source_path: &Path,
    roots: &[PathBuf],
) -> Result<bool, String> {
    let validated_source = canonicalize_existing_path(source_path, "session source")?;

    let mut saw_existing_root = false;
    for root in roots {
        if !root.exists() {
            continue;
        }

        saw_existing_root = true;
        let validated_root = canonicalize_existing_path(root, "session root")?;
        if validated_source.starts_with(&validated_root) {
            return match provider_id {
                "codex" => codex::delete_session(&validated_root, &validated_source, session_id),
                "claude" => claude::delete_session(&validated_root, &validated_source, session_id),
                "opencode" => {
                    opencode::delete_session(&validated_root, &validated_source, session_id)
                }
                "openclaw" => {
                    openclaw::delete_session(&validated_root, &validated_source, session_id)
                }
                "gemini" => gemini::delete_session(&validated_root, &validated_source, session_id),
                "hermes" => hermes::delete_session(&validated_root, &validated_source, session_id),
                _ => Err(format!("Unsupported provider: {provider_id}")),
            };
        }
    }

    if !saw_existing_root {
        return Err(format!(
            "Session root not found for provider {provider_id}: {}",
            roots
                .first()
                .map(|root| root.display().to_string())
                .unwrap_or_else(|| "<none>".to_string())
        ));
    }

    Err(format!(
        "Session source path is outside provider roots: {}",
        source_path.display()
    ))
}

fn provider_roots(provider_id: &str) -> Result<Vec<PathBuf>, String> {
    let roots = match provider_id {
        "codex" => codex::session_roots(),
        "claude" => vec![crate::config::get_claude_config_dir().join("projects")],
        "opencode" => vec![opencode::get_opencode_data_dir()],
        "codefree-o" => vec![crate::settings::get_codefree_o_home_dir()
            .join(".local")
            .join("share")
            .join("codefree.db")],
        "openclaw" => vec![crate::openclaw_config::get_openclaw_dir().join("agents")],
        "gemini" => vec![crate::gemini_config::get_gemini_dir().join("tmp")],
        "hermes" => vec![crate::hermes_config::get_hermes_dir().join("sessions")],
        _ => return Err(format!("Unsupported provider: {provider_id}")),
    };

    Ok(roots)
}

fn canonicalize_existing_path(path: &Path, label: &str) -> Result<PathBuf, String> {
    if !path.exists() {
        return Err(format!("{label} not found: {}", path.display()));
    }

    path.canonicalize()
        .map_err(|e| format!("Failed to resolve {label} {}: {e}", path.display()))
}

fn collect_delete_session_outcomes<F>(
    requests: &[DeleteSessionRequest],
    mut deleter: F,
) -> Vec<DeleteSessionOutcome>
where
    F: FnMut(&DeleteSessionRequest) -> Result<bool, String>,
{
    requests
        .iter()
        .map(|request| match deleter(request) {
            Ok(true) => DeleteSessionOutcome {
                provider_id: request.provider_id.clone(),
                session_id: request.session_id.clone(),
                source_path: request.source_path.clone(),
                success: true,
                error: None,
            },
            Ok(false) => DeleteSessionOutcome {
                provider_id: request.provider_id.clone(),
                session_id: request.session_id.clone(),
                source_path: request.source_path.clone(),
                success: false,
                error: Some("Session was not deleted".to_string()),
            },
            Err(error) => DeleteSessionOutcome {
                provider_id: request.provider_id.clone(),
                session_id: request.session_id.clone(),
                source_path: request.source_path.clone(),
                success: false,
                error: Some(error),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn codefree_dispatch_env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn create_codefree_sqlite_schema(conn: &Connection) {
        conn.execute_batch(
            r#"
            CREATE TABLE session (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                directory TEXT NOT NULL,
                time_created INTEGER NOT NULL,
                time_updated INTEGER NOT NULL
            );
            CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                time_created INTEGER NOT NULL,
                data TEXT NOT NULL
            );
            CREATE TABLE part (
                id TEXT PRIMARY KEY,
                message_id TEXT NOT NULL,
                session_id TEXT NOT NULL,
                time_created INTEGER NOT NULL,
                data TEXT NOT NULL
            );
            "#,
        )
        .expect("create sqlite schema");
    }

    fn write_codex_session(path: &Path, session_id: &str) {
        std::fs::write(
            path,
            format!(
                "{{\"timestamp\":\"2026-03-06T21:50:12Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{session_id}\",\"cwd\":\"/tmp/project\"}}}}\n\
                 {{\"timestamp\":\"2026-03-06T21:50:13Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"user\",\"content\":\"hello\"}}}}\n",
            ),
        )
        .expect("write source");
    }

    #[test]
    fn accepts_source_path_under_any_allowed_provider_root() {
        let active_root = tempdir().expect("active root");
        let archived_root = tempdir().expect("archived root");
        let source = archived_root.path().join("session.jsonl");
        write_codex_session(&source, "archived-session");

        let deleted = delete_session_with_roots(
            "codex",
            "archived-session",
            &source,
            &[
                active_root.path().to_path_buf(),
                archived_root.path().to_path_buf(),
            ],
        )
        .expect("delete archived session");

        assert!(deleted);
        assert!(!source.exists());
    }

    #[test]
    fn rejects_source_path_outside_provider_root() {
        let root = tempdir().expect("tempdir");
        let outside = tempdir().expect("tempdir");
        let source = outside.path().join("session.jsonl");
        std::fs::write(&source, "{}").expect("write source");

        let err =
            delete_session_with_roots("codex", "session-1", &source, &[root.path().to_path_buf()])
                .expect_err("expected outside-root path to be rejected");

        assert!(err.contains("outside provider roots"));
    }

    #[test]
    fn rejects_missing_source_path() {
        let root = tempdir().expect("tempdir");
        let missing = root.path().join("missing.jsonl");

        let err =
            delete_session_with_roots("codex", "session-1", &missing, &[root.path().to_path_buf()])
                .expect_err("expected missing source path to fail");

        assert!(err.contains("session source not found"));
    }

    #[test]
    fn batch_delete_collects_successes_and_failures_in_order() {
        let requests = vec![
            DeleteSessionRequest {
                provider_id: "codex".to_string(),
                session_id: "s1".to_string(),
                source_path: "/tmp/s1".to_string(),
            },
            DeleteSessionRequest {
                provider_id: "claude".to_string(),
                session_id: "s2".to_string(),
                source_path: "/tmp/s2".to_string(),
            },
            DeleteSessionRequest {
                provider_id: "gemini".to_string(),
                session_id: "s3".to_string(),
                source_path: "/tmp/s3".to_string(),
            },
        ];

        let outcomes = collect_delete_session_outcomes(&requests, |request| {
            match request.session_id.as_str() {
                "s1" => Ok(true),
                "s2" => Err("boom".to_string()),
                _ => Ok(false),
            }
        });

        assert_eq!(outcomes.len(), 3);
        assert!(outcomes[0].success);
        assert_eq!(outcomes[0].error, None);
        assert!(!outcomes[1].success);
        assert_eq!(outcomes[1].error.as_deref(), Some("boom"));
        assert!(!outcomes[2].success);
        assert_eq!(
            outcomes[2].error.as_deref(),
            Some("Session was not deleted")
        );
    }

    #[test]
    #[allow(deprecated)]
    fn load_messages_dispatches_codefree_o_sqlite_sessions() {
        let _guard = codefree_dispatch_env_lock().lock().expect("lock");
        let temp = tempfile::tempdir().expect("tempdir");
        let original_test_home = std::env::var_os("CC_SWITCH_TEST_HOME");
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path());

        let db_dir = temp.path().join(".codefree-o").join(".local").join("share");
        std::fs::create_dir_all(&db_dir).expect("create db dir");
        let db_path = db_dir.join("codefree.db");
        let conn = Connection::open(&db_path).expect("open sqlite db");
        create_codefree_sqlite_schema(&conn);
        conn.execute(
            "INSERT INTO session (id, title, directory, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5)",
            ("ses_1", "Session", "/tmp/project-a", 1000_i64, 2000_i64),
        )
        .expect("insert session");
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            ("msg_1", "ses_1", 1000_i64, r#"{"role":"user"}"#),
        )
        .expect("insert message");
        conn.execute(
            "INSERT INTO part (id, session_id, message_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            ("prt_1", "ses_1", "msg_1", 1000_i64, r#"{"type":"text","text":"hello dispatch"}"#),
        )
        .expect("insert part");
        drop(conn);

        let source = format!("sqlite:{}:ses_1", db_path.display());
        let result = load_messages("codefree-o", &source).expect("load messages");

        if let Some(value) = original_test_home {
            std::env::set_var("CC_SWITCH_TEST_HOME", value);
        } else {
            std::env::remove_var("CC_SWITCH_TEST_HOME");
        }

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, "hello dispatch");
    }

    #[test]
    #[allow(deprecated)]
    fn delete_session_dispatches_codefree_o_sqlite_sessions() {
        let _guard = codefree_dispatch_env_lock().lock().expect("lock");
        let temp = tempfile::tempdir().expect("tempdir");
        let original_test_home = std::env::var_os("CC_SWITCH_TEST_HOME");
        std::env::set_var("CC_SWITCH_TEST_HOME", temp.path());

        let db_dir = temp.path().join(".codefree-o").join(".local").join("share");
        std::fs::create_dir_all(&db_dir).expect("create db dir");
        let db_path = db_dir.join("codefree.db");
        let conn = Connection::open(&db_path).expect("open sqlite db");
        create_codefree_sqlite_schema(&conn);
        conn.execute(
            "INSERT INTO session (id, title, directory, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5)",
            ("ses_1", "Session", "/tmp/project-a", 1000_i64, 2000_i64),
        )
        .expect("insert session");
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            ("msg_1", "ses_1", 1000_i64, r#"{"role":"user"}"#),
        )
        .expect("insert message");
        conn.execute(
            "INSERT INTO part (id, session_id, message_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            ("prt_1", "ses_1", "msg_1", 1000_i64, r#"{"type":"text","text":"Hello"}"#),
        )
        .expect("insert part");
        drop(conn);

        let source = format!("sqlite:{}:ses_1", db_path.display());
        let deleted = delete_session("codefree-o", "ses_1", &source).expect("delete session");

        if let Some(value) = original_test_home {
            std::env::set_var("CC_SWITCH_TEST_HOME", value);
        } else {
            std::env::remove_var("CC_SWITCH_TEST_HOME");
        }

        assert!(deleted);
    }
}
