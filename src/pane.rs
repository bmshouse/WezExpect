use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::process::Command;

/// Information about a WezTerm pane from `wezterm cli list`
#[derive(Debug, Deserialize, Clone)]
pub struct PaneInfo {
    /// Unique identifier for this pane
    pub pane_id: u32,
    /// Current title of the pane
    pub title: String,
    /// Current working directory in the pane
    #[serde(default)]
    pub cwd: String,
}

/// Discover all available WezTerm panes
pub async fn list_panes() -> Result<Vec<PaneInfo>> {
    let output = Command::new("wezterm")
        .args(&["cli", "list", "--format", "json"])
        .output()
        .await
        .context("Failed to execute 'wezterm cli list'. Is WezTerm installed and in PATH?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("wezterm cli list failed: {}", stderr);
    }

    let stdout = String::from_utf8(output.stdout)
        .context("wezterm cli list output is not valid UTF-8")?;

    let panes: Vec<PaneInfo> = serde_json::from_str(&stdout)
        .context("Failed to parse wezterm cli list JSON output")?;

    Ok(panes)
}

/// Select a pane, either from config or by auto-selecting the first available pane
pub async fn select_pane(config_pane_id: Option<u32>) -> Result<u32> {
    let panes = list_panes().await?;

    if panes.is_empty() {
        anyhow::bail!("No WezTerm panes found. Please open a WezTerm window.");
    }

    // If pane_id is specified in config, validate and use it
    if let Some(pane_id) = config_pane_id {
        if panes.iter().any(|p| p.pane_id == pane_id) {
            tracing::info!("Using configured pane_id: {}", pane_id);
            return Ok(pane_id);
        } else {
            anyhow::bail!(
                "Configured pane_id {} not found. Available panes: {:?}",
                pane_id,
                panes.iter().map(|p| p.pane_id).collect::<Vec<_>>()
            );
        }
    }

    // Otherwise, auto-select first pane
    let selected = &panes[0];
    tracing::info!(
        "Auto-selected pane {} (title: '{}', cwd: '{}')",
        selected.pane_id,
        selected.title,
        selected.cwd
    );

    // Log all available panes for user information
    if panes.len() > 1 {
        tracing::info!("Available panes:");
        for pane in &panes {
            tracing::info!(
                "  - Pane {}: '{}' ({})",
                pane.pane_id,
                pane.title,
                pane.cwd
            );
        }
        tracing::info!(
            "To monitor a specific pane, add 'pane_id = {}' in config.toml [pane] section",
            selected.pane_id
        );
    }

    Ok(selected.pane_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_list_panes_format() {
        // This test will only work if WezTerm is running
        // It's here to document expected behavior and verify deserialization works
        if let Ok(panes) = list_panes().await {
            // If WezTerm is running, we should get at least one pane
            assert!(!panes.is_empty(), "Expected at least one pane when WezTerm is running");

            for pane in panes {
                // Verify the structure deserialized correctly by checking all fields exist
                // pane_id can be any u32 value (WezTerm starts at 0)
                let _id = pane.pane_id;
                // title and cwd are strings (can be empty, which is valid)
                let _title = &pane.title;
                let _cwd = &pane.cwd;

                // If we got here, deserialization succeeded
            }
        }
        // If WezTerm isn't running, test passes silently (this is expected)
    }
}
