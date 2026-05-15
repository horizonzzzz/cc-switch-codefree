use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::Connection;
use serde_json::Value;

use crate::session_manager::SessionMessage;

pub fn parse_family_sqlite_source(source: &str) -> Option<(PathBuf, String)> {
    let rest = source.strip_prefix("sqlite:")?;
    let sep = rest.rfind(":ses_")?;
    let db_path = PathBuf::from(&rest[..sep]);
    let session_id = rest[sep + 1..].to_string();
    Some((db_path, session_id))
}

pub fn load_family_messages_sqlite(source: &str) -> Result<Vec<SessionMessage>, String> {
    let (db_path, session_id) = parse_family_sqlite_source(source)
        .ok_or_else(|| format!("Invalid SQLite source reference: {source}"))?;

    let conn = Connection::open_with_flags(
        &db_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|e| format!("Failed to open family database: {e}"))?;

    let mut msg_stmt = conn
        .prepare(
            "SELECT id, time_created, data FROM message WHERE session_id = ?1 ORDER BY time_created ASC",
        )
        .map_err(|e| format!("Failed to prepare message query: {e}"))?;

    let msg_rows = msg_stmt
        .query_map([session_id.as_str()], |row| {
            let id: String = row.get(0)?;
            let ts: i64 = row.get(1)?;
            let data: String = row.get(2)?;
            Ok((id, ts, data))
        })
        .map_err(|e| format!("Failed to query messages: {e}"))?;

    let mut part_stmt = conn
        .prepare(
            "SELECT message_id, data FROM part WHERE session_id = ?1 ORDER BY time_created ASC",
        )
        .map_err(|e| format!("Failed to prepare part query: {e}"))?;

    let part_rows = part_stmt
        .query_map([session_id.as_str()], |row| {
            let message_id: String = row.get(0)?;
            let data: String = row.get(1)?;
            Ok((message_id, data))
        })
        .map_err(|e| format!("Failed to query parts: {e}"))?;

    let mut parts_map: HashMap<String, Vec<String>> = HashMap::new();
    for row in part_rows.flatten() {
        let (message_id, data) = row;
        parts_map.entry(message_id).or_default().push(data);
    }

    let mut messages = Vec::new();
    for row in msg_rows.flatten() {
        let (message_id, ts, data) = row;
        let msg_value: Value = match serde_json::from_str(&data) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let role = msg_value
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let mut texts = Vec::new();
        if let Some(parts) = parts_map.get(&message_id) {
            for part_data in parts {
                let part_value: Value = match serde_json::from_str(part_data) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if let Some(text) = extract_part_text(&part_value) {
                    texts.push(text);
                }
            }
        }

        let content = texts.join("\n");
        if content.trim().is_empty() {
            continue;
        }

        messages.push(SessionMessage {
            role,
            content,
            ts: Some(ts),
        });
    }

    Ok(messages)
}

pub fn delete_family_session_sqlite(
    session_id: &str,
    source: &str,
    expected_db_path: &PathBuf,
    provider_name: &str,
) -> Result<bool, String> {
    let (db_path, ref_session_id) = parse_family_sqlite_source(source)
        .ok_or_else(|| format!("Invalid SQLite source reference: {source}"))?;
    let db_path = db_path
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize SQLite database path: {e}"))?;
    let expected_db_path = expected_db_path
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize expected {provider_name} database path: {e}"))?;

    if ref_session_id != session_id {
        return Err(format!(
            "{provider_name} SQLite session ID mismatch: expected {session_id}, found {ref_session_id}"
        ));
    }
    if db_path != expected_db_path {
        return Err(format!("SQLite path does not match expected {provider_name} database"));
    }

    let conn = Connection::open(&db_path)
        .map_err(|e| format!("Failed to open {provider_name} database: {e}"))?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("Failed to begin transaction: {e}"))?;

    tx.execute("DELETE FROM part WHERE session_id = ?1", [session_id])
        .map_err(|e| format!("Failed to delete {provider_name} parts: {e}"))?;
    tx.execute("DELETE FROM message WHERE session_id = ?1", [session_id])
        .map_err(|e| format!("Failed to delete {provider_name} messages: {e}"))?;
    let deleted = tx
        .execute("DELETE FROM session WHERE id = ?1", [session_id])
        .map_err(|e| format!("Failed to delete {provider_name} session: {e}"))?;

    tx.commit()
        .map_err(|e| format!("Failed to commit session deletion: {e}"))?;

    Ok(deleted > 0)
}

pub fn extract_family_title(title: &str, directory: &str) -> Option<String> {
    if !title.is_empty() {
        return Some(title.to_string());
    }
    directory
        .trim_end_matches(['/', '\\'])
        .split(['/', '\\'])
        .next_back()
        .filter(|segment| !segment.is_empty())
        .map(|segment| segment.to_string())
}

fn extract_part_text(part_value: &Value) -> Option<String> {
    match part_value.get("type").and_then(Value::as_str) {
        Some("text") => part_value
            .get("text")
            .and_then(Value::as_str)
            .filter(|text| !text.trim().is_empty())
            .map(|text| text.to_string()),
        Some("tool") => {
            let tool = part_value
                .get("tool")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            Some(format!("[Tool: {tool}]"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::tempdir;

    fn create_sqlite_schema(conn: &Connection) {
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

    #[test]
    fn parse_family_sqlite_source_accepts_valid_references() {
        let parsed =
            parse_family_sqlite_source("sqlite:/tmp/family.db:ses_123").expect("valid source");

        assert_eq!(parsed.0, PathBuf::from("/tmp/family.db"));
        assert_eq!(parsed.1, "ses_123");
    }

    #[test]
    fn parse_family_sqlite_source_rejects_invalid_references() {
        assert!(parse_family_sqlite_source("/tmp/family.db:ses_123").is_none());
        assert!(parse_family_sqlite_source("sqlite:/tmp/family.db:msg_123").is_none());
        assert!(parse_family_sqlite_source("sqlite:/tmp/family.db").is_none());
    }

    #[test]
    fn load_family_messages_sqlite_reconstructs_text_and_tool_parts() {
        let temp = tempdir().expect("tempdir");
        let db_path = temp.path().join("family.db");
        let conn = Connection::open(&db_path).expect("open sqlite db");
        create_sqlite_schema(&conn);

        conn.execute(
            "INSERT INTO session (id, title, directory, time_created, time_updated) VALUES (?1, ?2, ?3, ?4, ?5)",
            ("ses_1", "Session", "/tmp/project-a", 1000_i64, 3000_i64),
        )
        .expect("insert session");
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            ("msg_1", "ses_1", 1000_i64, r#"{"role":"user"}"#),
        )
        .expect("insert message 1");
        conn.execute(
            "INSERT INTO message (id, session_id, time_created, data) VALUES (?1, ?2, ?3, ?4)",
            ("msg_2", "ses_1", 2000_i64, r#"{"role":"assistant"}"#),
        )
        .expect("insert message 2");
        conn.execute(
            "INSERT INTO part (id, session_id, message_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            ("prt_1", "ses_1", "msg_1", 1000_i64, r#"{"type":"text","text":"Hello"}"#),
        )
        .expect("insert part 1");
        conn.execute(
            "INSERT INTO part (id, session_id, message_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            ("prt_2", "ses_1", "msg_2", 2000_i64, r#"{"type":"tool","tool":"bash"}"#),
        )
        .expect("insert part 2");
        conn.execute(
            "INSERT INTO part (id, session_id, message_id, time_created, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            ("prt_3", "ses_1", "msg_2", 2001_i64, r#"{"type":"text","text":"Done"}"#),
        )
        .expect("insert part 3");
        drop(conn);

        let source = format!("sqlite:{}:ses_1", db_path.display());
        let messages = load_family_messages_sqlite(&source).expect("load sqlite messages");

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].role, "user");
        assert_eq!(messages[0].content, "Hello");
        assert_eq!(messages[0].ts, Some(1000));
        assert_eq!(messages[1].role, "assistant");
        assert_eq!(messages[1].content, "[Tool: bash]\nDone");
        assert_eq!(messages[1].ts, Some(2000));
    }
}
