use anyhow::Context;
use iced::futures::{Stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio_util::{
    codec::{FramedRead, LinesCodec},
    io::StreamReader,
};

use rig_core::OneOrMany;
use rig_core::client::{CompletionClient, ModelListingClient};
use rig_core::completion::message::{AssistantContent, UserContent};
use rig_core::completion::{CompletionModel, CompletionRequest, Message};
use rig_core::providers::openai;
use rig_core::streaming::{StreamedAssistantContent, StreamingCompletionResponse};

use crate::{ai_config::AIConfig, components::ai_chat::Role};

pub async fn list_models(config: &AIConfig) -> Result<Vec<String>, String> {
    let base_url = {
        let url = config.endpoint.trim_end_matches('/');
        if url.contains("/v1") {
            url.to_string()
        } else {
            format!("{url}/v1")
        }
    };

    let client = openai::Client::builder()
        .api_key(config.api_key.clone().unwrap_or_default())
        .base_url(&base_url)
        .build()
        .map_err(|e| format!("Failed to build OpenAI client: {e}"))?;

    let models = client
        .list_models()
        .await
        .map_err(|e| format!("Failed to list models: {e}"))?;

    Ok(models.iter().map(|m| m.id.clone()).collect())
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
        }
    }
}

pub async fn prompt_ollama(
    config: AIConfig,
    prompt: Vec<ChatMessage>,
) -> anyhow::Result<impl Stream<Item = anyhow::Result<ChatResponseChunk>>> {
    let url = format!("{}/api/chat", config.endpoint);
    let mut builder = reqwest::Client::new().post(&url);
    if let Some(ref key) = config.api_key {
        builder = builder.header("Authorization", format!("Bearer {key}"));
    }
    let body = ChatRequest {
        model: config.model.to_string(),
        messages: prompt,
        stream: true,
    };

    let response = builder.json(&body).send().await?.error_for_status()?;
    let stream = response
        .bytes_stream()
        .map_err(|err| std::io::Error::new(std::io::ErrorKind::Other, err));
    let reader = StreamReader::new(stream);
    let lines_stream = FramedRead::new(reader, LinesCodec::new());
    let parsed_stream = lines_stream.filter_map(|line_result| async move {
        match line_result {
            Ok(line) => {
                if line.trim().is_empty() {
                    None
                } else {
                    Some(
                        serde_json::from_str::<ChatResponseChunk>(&line)
                            .context("Failed to parse model response chunk from stream"),
                    )
                }
            }
            Err(err) => Some(Err(err).context("Failed to read line from stream")),
        }
    });
    Ok(parsed_stream)
}

pub async fn prompt(
    config: AIConfig,
    prompt: Vec<ChatMessage>,
) -> anyhow::Result<impl Stream<Item = anyhow::Result<ChatResponseChunk>>> {
    eprintln!(
        "[pgeru] prompt: config={{ endpoint: {}, model: {}, has_api_key: {} }} prompt_len: {}",
        config.endpoint,
        config.model,
        config.api_key.is_some(),
        prompt.len()
    );

    let api_key = config.api_key.clone().unwrap_or_default();

    let base_url = {
        let url = config.endpoint.trim_end_matches('/');
        if url.contains("/v1") {
            url.to_string()
        } else {
            format!("{url}/v1")
        }
    };
    eprintln!("[pgeru] prompt: base_url={}", base_url);

    let client = openai::Client::builder()
        .api_key(api_key)
        .base_url(&base_url)
        .build()
        .context("Failed to build OpenAI client")?
        .completions_api();

    eprintln!("[pgeru] prompt: openai client built");

    let model = client.completion_model(&config.model);

    eprintln!(
        "[pgeru] prompt: completion model created for model={}",
        config.model
    );

    let messages: Vec<Message> = prompt.into_iter().map(|m| m.into()).collect();
    eprintln!(
        "[pgeru] prompt: converted {} messages to rig-core format",
        messages.len()
    );

    let request = CompletionRequest {
        model: Some(String::from("deepseek-v4-flash-free")),
        preamble: Some(String::from("You are the core AI intelligence engine integrated into a native PostgreSQL GUI desktop client. Your primary objective is to assist developers and database administrators in safely writing, optimizing, and understanding PostgreSQL queries.
        Follow these strict operational constraints:
        1. SQL Generation: Always generate clean, idiomatic PostgreSQL syntax. Capitalize SQL keywords (e.g., SELECT, JOIN, WHERE, GROUP BY).
        2. Safety & Destructive Actions: If the user asks for a destructive operation (DROP, TRUNCATE, DELETE without a WHERE clause), you must wrap the SQL code block, explicitly warn them of the data loss risk, and suggest using a TRANSACTION (BEGIN; ... ROLLBACK/COMMIT;) for safety.
        3. Schema Awareness: Assume standard PostgreSQL data types and features (such as JSONB, UUIDs, window functions, and CTEs) are fully available.
        4. Formatting: When returning SQL, always format it within standard markdown code blocks tagged as ```sql. Keep explanations brief, technical, and precise.
        5. Content Isolation: Never include markdown formatting, conversational filler, or prose inside the ```sql code block itself—keep the code completely raw and ready to execute.")),
        chat_history: OneOrMany::many(messages).context("Chat history cannot be empty")?,
        documents: vec![],
        tools: vec![],
        temperature: None,
        max_tokens: None,
        tool_choice: None,
        additional_params: None,
        output_schema: None,
    };

    eprintln!("[pgeru] prompt: calling model.stream()...");
    let stream: StreamingCompletionResponse<_> = match model.stream(request).await {
        Ok(s) => {
            eprintln!("[pgeru] prompt: stream started successfully");
            s
        }
        Err(err) => {
            eprintln!("[pgeru] prompt: stream failed: {err}");
            return Err(anyhow::anyhow!(
                "Failed to start streaming completion: {err}"
            ));
        }
    };

    let model_name = config.model.clone();
    let mapped = stream.map(move |item| {
        item.map(|content| match content {
            StreamedAssistantContent::Text(text) => {
                ChatResponseChunk::Message(ChatResponseMessage::Content(text.text))
            }
            StreamedAssistantContent::ToolCall {
                tool_call,
                internal_call_id,
            } => todo!(),
            StreamedAssistantContent::ToolCallDelta {
                id,
                internal_call_id,
                content,
            } => todo!(),
            StreamedAssistantContent::Reasoning(reasoning) => todo!(),

            StreamedAssistantContent::ReasoningDelta { reasoning, .. } => {
                eprintln!(
                    "[pgeru] prompt: [req_model={}] chunk ReasoningDelta len={}",
                    model_name,
                    reasoning.len()
                );
                ChatResponseChunk::Message(ChatResponseMessage::Thinking(reasoning))
            }

            StreamedAssistantContent::Final(response) => {
                let extra = serde_json::to_string(&response).unwrap_or_else(|_| "N/A".into());
                eprintln!(
                    "[pgeru] prompt: [req_model={}] chunk Final response_json={}",
                    model_name, extra
                );
                ChatResponseChunk::Done
            }
        })
        .map_err(Into::into)
    });

    Ok(mapped)
}
