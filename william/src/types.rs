use rusqlite::params;

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

    pub fn id(&self) -> i64 {
        match self {
            MessageType::System => 0,
            MessageType::User => 1,
            MessageType::Assistant => 2,
        }
    }

    pub fn from_id(id: i64) -> Result<Self, String> {
        match id {
            0 => Ok(MessageType::System),
            1 => Ok(MessageType::User),
            2 => Ok(MessageType::Assistant),
            _ => Err(format!("Invalid message type: {}", id)),
        }
    }
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Message {
    pub id: Option<i64>,
    pub message_type: MessageType,
    pub content: String,
    pub model: String,
    pub system_prompt: String,
    pub sequence: i32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Conversation {
    pub id: Option<i64>,
    pub name: String,
    pub messages: Vec<Message>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SystemPrompt {
    pub write: bool,
    pub content: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ConversationList {
    pub conversations: Vec<Conversation>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct LoadConversation {
    pub id: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Ping {
    pub body: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum RequestPayload {
    Ping(Ping),
    Completion(Conversation),
    ConversationList,
    Load(LoadConversation),
    SystemPrompt(SystemPrompt),
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "method")]
pub enum ArrakisRequest {
    Ping { payload: Ping },
    Completion { payload: Conversation },
    ConversationList,
    Load { payload: LoadConversation },
    SystemPrompt { payload: SystemPrompt },
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
pub struct Completion {
    pub stream: bool,
    pub delta: String,
    pub name: String,
    #[serde(rename = "conversationId")]
    pub conversation_id: i64,
    #[serde(rename = "requestId")]
    pub request_id: i64,
    #[serde(rename = "responseId")]
    pub response_id: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ArrakisResponse {
    pub payload: ResponsePayload,
}

// search.rs (for Dewey-related structures)
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

// config.rs
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

impl Message {
    pub fn update(&self, db: &rusqlite::Connection) -> rusqlite::Result<usize> {
        db.execute(
            "UPDATE messages SET content = ?2 WHERE id = ?1",
            params![self.id, self.content],
        )
    }

    pub fn insert(&mut self, db: &rusqlite::Connection) -> rusqlite::Result<usize> {
        let update_count = db.execute(
            "INSERT INTO messages (message_type_id, content, model, system_prompt) VALUES (?1, ?2, ?3, ?4)",
            params![self.message_type.id(), self.content, self.model, self.system_prompt],
        );

        self.id = Some(db.last_insert_rowid());

        update_count
    }

    pub fn upsert(&mut self, db: &rusqlite::Connection) -> rusqlite::Result<usize> {
        if self.update(db)? == 0 {
            self.insert(db)
        } else {
            Ok(1)
        }
    }
}

impl Conversation {
    // the IDs of all objects _need_ to be set before leaving this function
    // basic order here is something like:
    // - upsert conversation table
    // - upsert each message item (for setting IDs + updating contents)
    // - reset links
    pub fn upsert(&mut self, db: &rusqlite::Connection) -> rusqlite::Result<usize> {
        if self.id.is_none() {
            db.execute(
                "INSERT INTO conversations (name) VALUES (?1)",
                params![self.name],
            )?;

            self.id = Some(db.last_insert_rowid());
        } else {
            db.execute(
                "UPDATE conversations SET name = ?2 WHERE id = ?1",
                params![self.id, self.name],
            )?;
        }

        for message in self.messages.iter_mut() {
            message.upsert(db)?;
        }

        db.execute(
            "DELETE FROM links WHERE conversation_id = ?1",
            params![self.id],
        )?;

        for (sequence, message) in self.messages.iter().enumerate() {
            db.execute(
                "INSERT INTO links (conversation_id, message_id, sequence) VALUES (?1, ?2, ?3)",
                params![self.id, message.id, sequence as i64],
            )?;
        }

        Ok(1)
    }
}
