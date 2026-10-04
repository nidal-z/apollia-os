//! The `code_context` tool of a run, granted by the code folders its agent
//! has checked.
//!
//! The agent's access never grants the tool on its own: it is offered, and
//! its executor bound to the folders, exactly when a code folder is checked
//! for the agent. Nothing of the folder is put in the prompt.

use apollia_tools::executor::ToolExecutor;

use super::ChatToolsConfig;

/// `tools` with `code_context` present exactly when `agent` has a code
/// folder checked, its executor then pushed to `executors`. Without a tools
/// configuration there is no data directory to read the sources from, and no
/// code folder.
pub(in crate::chat::manager) async fn with_code_context(
    cfg: Option<&ChatToolsConfig>,
    agent: &str,
    tools: Vec<String>,
    executors: &mut Vec<Box<dyn ToolExecutor>>,
) -> Vec<String> {
    let tool = match cfg {
        Some(cfg) => {
            let data_dir = cfg.data_dir.clone();
            let agent = agent.to_owned();
            tokio::task::spawn_blocking(move || {
                crate::agents::sources::code_context_for(&data_dir, &agent)
            })
            .await
            .ok()
            .flatten()
        }
        None => None,
    };
    let tools = apollia_tools::granted_with_code_folders(tools, tool.is_some());
    if let Some(tool) = tool {
        executors.push(Box::new(tool));
    }
    tools
}
