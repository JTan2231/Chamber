#[derive(PartialEq, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub enum MessageType {
    System,
    User,
    Assistant,
}

impl MessageType {
    pub fn to_string(&self) -> String {
        match self {
            MessageType::System => "system".to_string(),
            MessageType::User => "user".to_string(),
            MessageType::Assistant => "assistant".to_string(),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub message_type: MessageType,
    pub content: String,
    pub model: String,
    pub system_prompt: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Conversation {
    pub name: String,
    pub conversation: Vec<Message>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ConversationList {
    pub conversations: Vec<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LoadConversation {
    pub name: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Ping {
    pub body: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SystemPrompt {
    pub write: bool,
    pub content: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "method")]
pub enum RequestPayload {
    Ping(Ping),
    Completion(Conversation),
    ConversationList,
    Load(LoadConversation),
    SystemPrompt(SystemPrompt),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ArrakisRequest {
    pub payload: RequestPayload,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Completion {
    pub stream: bool,
    pub delta: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "method")]
pub enum ResponsePayload {
    Ping(Ping),
    Completion(Completion),
    CompletionEnd,
    ConversationList(ConversationList),
    Load(Conversation),
    SystemPrompt(SystemPrompt),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ArrakisResponse {
    pub payload: ResponsePayload,
}

// these two are copied from Dewey
// this should really be all integrated into a single project
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DeweyRequest {
    pub k: usize,
    pub query: String,
    pub filters: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeweyResponseItem {
    pub filepath: String,
    pub subset: (u64, u64),
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DeweyResponse {
    pub results: Vec<DeweyResponseItem>,
}

#[derive(Clone, Debug)]
pub struct RequestParams {
    pub provider: String,
    pub host: String,
    pub path: String,
    pub port: u16,
    pub messages: Vec<Message>,
    pub model: String,
    pub stream: bool,
    pub authorization_token: String,
    pub max_tokens: Option<u16>,
    pub system_prompt: Option<String>,
}
