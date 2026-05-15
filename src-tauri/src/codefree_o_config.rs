use crate::config::write_json_file;
use crate::error::AppError;
use crate::settings::get_codefree_o_override_dir;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub fn get_codefree_o_dir() -> PathBuf {
    if let Some(override_dir) = get_codefree_o_override_dir() {
        return override_dir;
    }

    crate::config::get_home_dir()
        .join(".codefree-o")
        .join(".config")
}

pub fn get_codefree_o_config_path() -> PathBuf {
    get_codefree_o_dir().join("codefree.json")
}

pub fn read_codefree_o_config() -> Result<Value, AppError> {
    let path = get_codefree_o_config_path();

    if !path.exists() {
        return Ok(json!({
            "$schema": "https://opencode.ai/config.json"
        }));
    }

    let content = std::fs::read_to_string(&path).map_err(|e| AppError::io(&path, e))?;
    json5::from_str(&content).map_err(|e| {
        AppError::Config(format!(
            "Failed to parse codefree-o config: {}: {e}",
            path.display()
        ))
    })
}

pub fn write_codefree_o_config(config: &Value) -> Result<(), AppError> {
    let path = get_codefree_o_config_path();
    write_json_file(&path, config)?;
    Ok(())
}

pub fn get_mcp_servers() -> Result<Map<String, Value>, AppError> {
    let config = read_codefree_o_config()?;
    Ok(config
        .get("mcp")
        .and_then(|v| v.as_object())
        .cloned()
        .unwrap_or_default())
}

pub fn set_mcp_server(id: &str, config: Value) -> Result<(), AppError> {
    let mut full_config = read_codefree_o_config()?;

    if full_config.get("mcp").is_none() {
        full_config["mcp"] = json!({});
    }

    if let Some(mcp) = full_config.get_mut("mcp").and_then(|v| v.as_object_mut()) {
        mcp.insert(id.to_string(), config);
    }

    write_codefree_o_config(&full_config)
}

pub fn remove_mcp_server(id: &str) -> Result<(), AppError> {
    let mut config = read_codefree_o_config()?;

    if let Some(mcp) = config.get_mut("mcp").and_then(|v| v.as_object_mut()) {
        mcp.remove(id);
    }

    write_codefree_o_config(&config)
}
