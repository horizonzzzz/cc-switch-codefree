use serde_json::Value;
use std::collections::HashMap;

use crate::app_config::{McpApps, McpServer, MultiAppConfig};
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

pub fn import_from_codefree_o(config: &mut MultiAppConfig) -> Result<usize, AppError> {
    let mcp_map = codefree_o_config::get_mcp_servers()?;
    if mcp_map.is_empty() {
        return Ok(0);
    }

    let servers = config.mcp.servers.get_or_insert_with(HashMap::new);

    let mut changed = 0;
    let mut errors = Vec::new();

    for (id, spec) in mcp_map {
        let unified_spec = match convert_from_opencode_format(&spec) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("Skip invalid codefree-o MCP server '{id}': {e}");
                errors.push(format!("{id}: {e}"));
                continue;
            }
        };

        if let Err(e) = validate_server_spec(&unified_spec) {
            log::warn!("Skip invalid MCP server '{id}' after conversion: {e}");
            errors.push(format!("{id}: {e}"));
            continue;
        }

        if let Some(existing) = servers.get_mut(&id) {
            if !existing.apps.codefree_o {
                existing.apps.codefree_o = true;
                changed += 1;
                log::info!("MCP server '{id}' enabled for codefree-o");
            }
        } else {
            servers.insert(
                id.clone(),
                McpServer {
                    id: id.clone(),
                    name: id.clone(),
                    server: unified_spec,
                    apps: McpApps {
                        claude: false,
                        codex: false,
                        gemini: false,
                        opencode: false,
                        codefree_o: true,
                        hermes: false,
                    },
                    description: None,
                    homepage: None,
                    docs: None,
                    tags: Vec::new(),
                },
            );
            changed += 1;
            log::info!("Imported new MCP server '{id}' from codefree-o");
        }
    }

    if !errors.is_empty() {
        log::warn!(
            "Import completed with {} failures: {:?}",
            errors.len(),
            errors
        );
    }

    Ok(changed)
}
