use serde_json::Value;

use crate::app_config::MultiAppConfig;
use crate::codefree_o_config;
use crate::error::AppError;

use super::opencode::{convert_from_opencode_format, convert_to_opencode_format};
use super::validation::validate_server_spec;

fn should_sync_codefree_o_mcp() -> bool {
    codefree_o_config::get_codefree_o_dir().exists()
}

pub fn sync_single_server_to_codefree_o(
    _config: &MultiAppConfig,
    id: &str,
    server_spec: &Value,
) -> Result<(), AppError> {
    if !should_sync_codefree_o_mcp() {
        return Ok(());
    }

    let spec = convert_to_opencode_format(server_spec)?;
    codefree_o_config::set_mcp_server(id, spec)
}

pub fn remove_server_from_codefree_o(id: &str) -> Result<(), AppError> {
    if !should_sync_codefree_o_mcp() {
        return Ok(());
    }

    codefree_o_config::remove_mcp_server(id)
}

#[allow(dead_code)]
pub fn import_from_codefree_o(_config: &mut MultiAppConfig) -> Result<usize, AppError> {
    let _ = convert_from_opencode_format;
    let _ = validate_server_spec;
    Ok(0)
}
