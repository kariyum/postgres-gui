use serde::Deserialize;

use crate::ai_config::AiConfig;

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

pub async fn list_models(config: &AiConfig) -> Result<Vec<String>, String> {
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
