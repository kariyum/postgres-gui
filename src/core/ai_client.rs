use anyhow::Context;
use iced::futures::{Stream, StreamExt, TryStreamExt};
use serde::{Deserialize, Serialize};
use tokio_util::{
    codec::{Decoder, FramedRead, LinesCodec},
    io::StreamReader,
};

use crate::{ai_config::AIConfig, components::ai_chat::Role, core::event_stream_parser};

#[derive(Debug, Deserialize)]
pub struct OllamaModel {
    pub name: String,
    #[allow(dead_code)]
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct ModelsResponse {
    pub models: Vec<OllamaModel>,
}

pub async fn list_models(config: &AIConfig) -> Result<Vec<String>, String> {
    let url = format!("{}/api/tags", config.endpoint);
    let mut builder = reqwest::Client::new().get(&url);
    if let Some(ref key) = config.api_key {
        builder = builder.header("Authorization", format!("Bearer {key}"));
    }
    let resp = builder
        .send()
        .await
        .map_err(|e| format!("Request failed: {e}"))?;
    let body: ModelsResponse = resp
        .json()
        .await
        .map_err(|e| format!("Parse failed: {e}"))?;
    Ok(body.models.into_iter().map(|m| m.name).collect())
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
pub struct ChatResponseMessage {
    pub content: String,
    pub role: Role,
    pub thinking: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatResponseChunk {
    pub model: String,
    pub message: ChatResponseMessage,
    pub done: bool,
}

impl From<ZenChatResponseChunk> for ChatResponseChunk {
    fn from(value: ZenChatResponseChunk) -> Self {
        if let Some(choice) = value.choices.into_iter().next() {
            Self {
                model: String::from("NOT SET"),
                message: ChatResponseMessage {
                    content: choice.delta.content.unwrap_or_default(),
                    role: Role::Assistant,
                    thinking: choice.delta.reasoning_content,
                },
                done: choice.finish_reason.is_some(),
            }
        } else {
            Self {
                model: String::from("NOT SET"),
                message: ChatResponseMessage {
                    content: String::new(),
                    role: Role::Assistant,
                    thinking: None,
                },
                done: false,
            }
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ZenChatResponseChunk {
    id: String,
    choices: Vec<ZenChunkChoice>,
}

#[derive(Debug, Clone, Deserialize)]
struct ZenChunkChoice {
    delta: ZenChunkChoiceDelta,
    finish_reason: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ZenChunkChoiceDelta {
    reasoning_content: Option<String>,
    content: Option<String>,
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
    let url = String::from("https://opencode.ai/zen/v1/chat/completions");
    let mut builder = reqwest::Client::new()
        .post(&url)
        .header("Accept", "text/event-stream");
    let key = String::from("");
    builder = builder.header("Authorization", format!("Bearer {key}"));
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
        // eprintln!("got line_result {:?}", line_result);
        match line_result.map(event_stream_parser::parse) {
            Ok(Some(event_stream_parser::SSE::Data(line))) => Some(
                serde_json::from_str::<ZenChatResponseChunk>(&line)
                    .context("Failed to parse model response chunk from stream")
                    .map(ChatResponseChunk::from),
            ),
            Ok(None) => None,
            Err(err) => Some(Err(err).context("Failed to read line from stream")),
        }
    });
    Ok(parsed_stream)
}
