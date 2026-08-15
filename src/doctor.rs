use std::{collections::BTreeMap, path::Path};

use serde_json::{Value, json};

use crate::{
    config::rpc_url_from_env,
    model::LoadedConfig,
    plan::cli_identity,
    process::{command, command_exists, require_success},
};

fn version(command_name: &str, cwd: &Path) -> Option<String> {
    if !command_exists(command_name) {
        return None;
    }
    require_success(&command(&[command_name, "--version"]), cwd, None, false)
        .ok()
        .and_then(|result| {
            result
                .stdout
                .lines()
                .next()
                .map(str::trim)
                .map(str::to_owned)
        })
}

pub fn doctor(config: Option<&LoadedConfig>) -> Value {
    let cwd = config.map_or_else(
        || std::env::current_dir().unwrap_or_default(),
        |value| Path::new(&value.project_root).to_path_buf(),
    );
    let analyzer = config.and_then(|value| value.value.checks.static_analysis.first());
    let mut tools = BTreeMap::from([
        ("git".to_owned(), version("git", &cwd)),
        ("v4hook".to_owned(), Some(cli_identity())),
        ("forge".to_owned(), version("forge", &cwd)),
        ("cast".to_owned(), version("cast", &cwd)),
        ("anvil".to_owned(), version("anvil", &cwd)),
    ]);
    if let Some(analyzer) = analyzer {
        tools.insert(
            "staticAnalyzer".to_owned(),
            command_exists(analyzer).then(|| analyzer.clone()),
        );
    }
    let mut missing: Vec<String> = tools
        .iter()
        .filter(|(_, value)| value.is_none())
        .map(|(name, _)| name.clone())
        .collect();
    let rpc_configured = config.map(|value| {
        rpc_url_from_env(
            &value.value.network.rpc_url_env,
            Path::new(&value.project_root),
        )
        .is_ok()
    });
    if let (Some(config), Some(false)) = (config, rpc_configured) {
        let name = &config.value.network.rpc_url_env;
        missing.push(format!("RPC setting {name} in the environment or .env"));
    }
    json!({
        "ok": missing.is_empty() && rpc_configured.unwrap_or(true),
        "tools": tools,
        "missing": missing,
        "projectRoot": config.map(|value| value.project_root.clone()),
        "rpcConfigured": rpc_configured,
        "staticAnalysisCommand": config.map(|value| value.value.checks.static_analysis.clone()),
    })
}
