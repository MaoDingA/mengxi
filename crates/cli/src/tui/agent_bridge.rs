// tui/agent_bridge.rs — Bridge between CLI/TUI and the agent framework
//
// Creates the full agent pipeline (provider + tools + subagents),
// and runs the agent in a background tokio task with event forwarding.

use std::sync::{Arc, mpsc::Sender};

use mengxi_agent::{
    Agent, AgentEvent, AgentConfig,
    SubagentRuntime, ProviderFactory,
    ToolRegistry,
};
use mengxi_agent::llm::{
    ClaudeProvider, OpenAICompatProvider, LlmProvider,
};
use mengxi_agent::tools::{register_all, register_subagents};

// ---------------------------------------------------------------------------
// CliProviderFactory — concrete ProviderFactory for the CLI
// ---------------------------------------------------------------------------

/// Creates LLM providers based on CLI flags.
struct CliProviderFactory {
    provider_type: String,
    default_model: String,
}

impl CliProviderFactory {
    fn new(provider_type: &str, default_model: &str) -> Self {
        Self {
            provider_type: provider_type.to_string(),
            default_model: default_model.to_string(),
        }
    }
}

impl ProviderFactory for CliProviderFactory {
    fn create_provider(&self, model_override: Option<&str>) -> Box<dyn LlmProvider> {
        let model = model_override.unwrap_or(&self.default_model);
        match self.provider_type.as_str() {
            "claude" => {
                let key = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
                Box::new(ClaudeProvider::new(&key).with_model(model))
            }
            "ollama" => {
                Box::new(
                    OpenAICompatProvider::new("http://localhost:11434")
                        .with_model(model),
                )
            }
            _ => {
                let key = std::env::var("OPENAI_API_KEY").ok();
                let mut p = OpenAICompatProvider::new("https://api.openai.com/v1")
                    .with_model(model);
                if let Some(k) = key {
                    p = p.with_api_key(&k);
                }
                Box::new(p)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Agent creation
// ---------------------------------------------------------------------------

/// Create the parent agent's LLM provider.
fn create_main_provider(provider: &str, model: &str) -> Box<dyn LlmProvider> {
    let factory = CliProviderFactory::new(provider, model);
    factory.create_provider(None)
}

/// Build a fully-configured Agent with all tools and subagents.
fn create_agent(provider: &str, model: &str) -> Agent {
    // 1. Register base tools (12)
    let mut registry = ToolRegistry::new();
    register_all(&mut registry);

    // 2. Clone registry for SubagentRuntime (base tools only, no subagent tools)
    let subagent_base = registry.clone();
    let registry_arc = Arc::new(subagent_base);

    // 3. Create SubagentRuntime
    let factory = Arc::new(CliProviderFactory::new(provider, model));
    let subagent_rt = Arc::new(SubagentRuntime::new(factory, registry_arc, 3));

    // 4. Register subagent tools into the original registry (now 16 tools)
    register_subagents(&mut registry, subagent_rt);

    // 5. Create parent agent
    let main_provider = create_main_provider(provider, model);
    Agent::new(main_provider, registry).with_config(AgentConfig::default())
}

// ---------------------------------------------------------------------------
// Background task
// ---------------------------------------------------------------------------

/// Spawn the agent background task on the tokio runtime.
///
/// The agent processes user messages sequentially and forwards
/// AgentEvents to the TUI via the std::sync channel.
pub fn spawn_agent_task(
    rt: &tokio::runtime::Runtime,
    provider: &str,
    model: &str,
    user_msg_rx: tokio::sync::mpsc::UnboundedReceiver<String>,
    event_tx: Sender<AgentEvent>,
) {
    let provider = provider.to_string();
    let model = model.to_string();

    rt.spawn(async move {
        let mut agent = create_agent(&provider, &model);
        let mut event_rx = match agent.take_events() {
            Some(rx) => rx,
            None => {
                let _ = event_tx.send(AgentEvent::Error(
                    "Failed to acquire agent event receiver".into(),
                ));
                return;
            }
        };

        // Forward agent events from tokio channel to std::sync channel
        let fwd_tx = event_tx.clone();
        tokio::spawn(async move {
            while let Some(event) = event_rx.recv().await {
                if fwd_tx.send(event).is_err() {
                    break; // TUI closed
                }
            }
        });

        // Process user messages sequentially
        let mut user_msg_rx = user_msg_rx;
        while let Some(msg) = user_msg_rx.recv().await {
            if let Err(e) = agent.run(&msg).await {
                let _ = event_tx.send(AgentEvent::Error(e.to_string()));
            }
        }
    });
}
