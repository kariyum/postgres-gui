pub enum SSE {
    Data(String),
}

pub fn parse(message: String) -> Option<SSE> {
    if let Some((key, value)) = message.split_once(": ") {
        match key {
            "data" => Some(SSE::Data(String::from(value))),
            _ => None,
        }
    } else {
        None
    }
}
