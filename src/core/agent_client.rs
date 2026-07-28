use std::format;

use anyhow::Context;
use iced::futures::{Stream, StreamExt};
use serde::{Deserialize, Serialize};

use rig_core::client::{CompletionClient, ModelListingClient};
use rig_core::completion::message::{AssistantContent, UserContent};
use rig_core::completion::{CompletionModel, CompletionRequest, Message};
use rig_core::message::ReasoningContent;
use rig_core::providers::openai;
use rig_core::streaming::{
    StreamedAssistantContent, StreamingCompletionResponse, ToolCallDeltaContent,
};
use rig_core::{OneOrMany, model::ModelList};
use tracing::info;

use crate::components::chat_msg::Role;
use crate::core::{agent_tools::Tools, configured_provider::ConfiguredProvider};

pub async fn list_opencode_models(
    api_key: String,
    base_url: Option<String>,
) -> anyhow::Result<Vec<String>> {
    let mut client = openai::Client::builder();

    if let Some(base_url) = base_url {
        client = client.base_url(base_url);
    }

    let built_client = client
        .api_key(api_key)
        .build()
        .context("Failed to build OpenAI client")?;

    let model_list: ModelList = built_client
        .list_models()
        .await
        .context("Failed to list models")?;

    Ok(model_list.data.into_iter().map(|m| m.id).collect())
}

#[derive(Deserialize)]
struct AnthropicModelsResponse {
    data: Vec<AnthropicModel>,
}

#[derive(Deserialize)]
struct AnthropicModel {
    id: String,
}

pub async fn list_anthropic_models(api_key: String) -> anyhow::Result<Vec<String>> {
    let client = reqwest::Client::new();
    let resp = client
        .get("https://api.anthropic.com/v1/models")
        .header("x-api-key", &api_key)
        .send()
        .await
        .context("Failed to send Anthropic models request")?;

    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        anyhow::bail!("Anthropic models request failed ({status}): {body}");
    }

    let response: AnthropicModelsResponse = resp
        .json()
        .await
        .context("Failed to parse Anthropic models response")?;

    Ok(response.data.into_iter().map(|m| m.id).collect())
}

#[derive(Serialize, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ChatMessage {
    pub content: String,
    pub role: Role,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatResponseMessage {
    Content(String),
    Thinking(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatResponseChunk {
    Message(ChatResponseMessage),
    ToolCallStarted {
        call_id: String,
        tool_name: String,
        initial_args: String,
    },
    ToolCallDelta {
        call_id: String,
        args_delta: String,
    },
    ToolCallComplete {
        call_id: String,
        tool_name: String,
        args: String,
    },
    Done,
}

impl From<ChatMessage> for Message {
    fn from(msg: ChatMessage) -> Self {
        match msg.role {
            Role::User => Message::User {
                content: OneOrMany::one(UserContent::text(msg.content)),
            },
            Role::Assistant => Message::Assistant {
                id: None,
                content: OneOrMany::one(AssistantContent::text(msg.content)),
            },
            Role::System => Message::System {
                content: msg.content,
            },
            Role::Thinking => Message::Assistant {
                id: None,
                content: OneOrMany::one(AssistantContent::text(msg.content)),
            },
            Role::Tool => Message::User {
                content: OneOrMany::one(UserContent::text(msg.content)),
            },
        }
    }
}

fn build_preamble() -> String {
    format!("{} {}",
        "You are the core AI intelligence engine integrated into a native PostgreSQL GUI desktop client. Your primary objective is to assist developers and database administrators in safely writing, optimizing, and understanding PostgreSQL queries.
        Follow these strict operational constraints:
        1. SQL Generation: Always generate clean, idiomatic PostgreSQL syntax. Capitalize SQL keywords (e.g., SELECT, JOIN, WHERE, GROUP BY).
        2. Safety & Destructive Actions: If the user asks for a destructive operation (DROP, TRUNCATE, DELETE without a WHERE clause), you must wrap the SQL code block, explicitly warn them of the data loss risk, and suggest using a TRANSACTION (BEGIN; ... ROLLBACK/COMMIT;) for safety.
        3. Schema Awareness: Assume standard PostgreSQL data types and features (such as JSONB, UUIDs, window functions, and CTEs) are fully available.
        4. Formatting: When returning SQL, always format it within standard markdown code blocks tagged as ```sql. Keep explanations brief, technical, and precise.
        5. Content Isolation: Never include markdown formatting, conversational filler, or prose inside the ```sql code block itself—keep the code completely raw and ready to execute.

        Tool Usage:
        You have PostgreSQL database tools available. Use them to inspect the database schema and execute queries when asked about database contents. Destructive operations (INSERT, UPDATE, DELETE, DROP, TRUNCATE, ALTER, CREATE) will require user approval before execution.",
        ""
    )
}

pub async fn prompt(
    configured_provider: ConfiguredProvider,
    model: String,
    prompt: Vec<ChatMessage>,
    tool_manager: Tools,
) -> anyhow::Result<impl Stream<Item = anyhow::Result<ChatResponseChunk>>> {
    let mut client = openai::Client::builder();

    if let Some(ref base_url) = configured_provider.base_url {
        client = client.base_url(base_url);
    }

    let built_client = client
        .api_key(configured_provider.api_key)
        .build()
        .context("Failed to build OpenAI client")?
        .completions_api();

    info!("prompt: openai client built");

    let model_name = model.clone();
    let model = built_client.completion_model(&model);

    let messages: Vec<Message> = prompt.into_iter().map(|m| m.into()).collect();
    info!(
        "prompt: converted {} messages to rig-core format",
        messages.len()
    );

    let tool_definitions = tool_manager
        .definitions()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get tool definitions: {e}"))?;

    let preamble = build_preamble();

    let request = CompletionRequest {
        model: Some(model_name),
        preamble: Some(preamble),
        chat_history: OneOrMany::many(messages).context("Chat history cannot be empty")?,
        documents: vec![],
        tools: tool_definitions,
        temperature: None,
        max_tokens: None,
        tool_choice: None,
        additional_params: None,
        output_schema: None,
    };

    info!("prompt: calling model.stream()...");
    let stream: StreamingCompletionResponse<_> = model
        .stream(request)
        .await
        .context("Failed to start streaming completion")?;

    let mapped = stream.map(move |item| {
        item.map(|content| match content {
            StreamedAssistantContent::Text(text) => {
                ChatResponseChunk::Message(ChatResponseMessage::Content(text.text))
            }
            StreamedAssistantContent::ToolCall {
                tool_call,
                internal_call_id,
            } => {
                let initial_args = match tool_call.function.arguments {
                    serde_json::Value::String(s) => s,
                    other => other.to_string(),
                };
                ChatResponseChunk::ToolCallStarted {
                    call_id: internal_call_id,
                    tool_name: tool_call.function.name,
                    initial_args,
                }
            }
            StreamedAssistantContent::ToolCallDelta {
                id,
                internal_call_id,
                content,
            } => {
                let delta_text = match content {
                    ToolCallDeltaContent::Name(n) => n,
                    ToolCallDeltaContent::Delta(d) => d,
                };
                ChatResponseChunk::ToolCallDelta {
                    call_id: internal_call_id,
                    args_delta: delta_text,
                }
            }
            StreamedAssistantContent::Reasoning(reasoning) => {
                let text: String = reasoning
                    .content
                    .iter()
                    .map(|rc| match rc {
                        ReasoningContent::Text { text, .. } => text.as_str(),
                        ReasoningContent::Summary(s) => s.as_str(),
                        _ => "",
                    })
                    .collect();
                ChatResponseChunk::Message(ChatResponseMessage::Thinking(text))
            }

            StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                ChatResponseChunk::Message(ChatResponseMessage::Thinking(reasoning))
            }

            StreamedAssistantContent::Final(response) => {
                let extra = serde_json::to_string(&response).unwrap_or_else(|_| "N/A".into());
                ChatResponseChunk::Done
            }
        })
        .map_err(Into::into)
    });

    Ok(mapped)
}
