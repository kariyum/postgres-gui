#[derive(Clone, Debug)]
pub struct ConfiguredProvider {
    pub api_key: String,
    pub models: Vec<String>,
    pub base_url: Option<String>,
}
