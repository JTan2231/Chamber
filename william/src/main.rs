use rusqlite::params;

use chamber_common::{error, get_config_dir, get_local_dir, get_root_dir, lprint, Logger};

use crate::types::*;

mod network;
mod types;

fn create_if_nonexistent(path: &std::path::PathBuf) {
    if !path.exists() {
        match std::fs::create_dir_all(&path) {
            Ok(_) => (),
            Err(e) => panic!("Failed to create directory: {:?}, {}", path, e),
        };
    }
}

fn get_conversations_dir() -> std::path::PathBuf {
    let local = get_local_dir();
    local.join("conversations")
}

// TODO: a lot of this setup code needs abstracted to a common module
fn setup() {
    // TODO: better path config handling
    chamber_common::Workspace::new("/home/joey/.local/william");

    create_if_nonexistent(&get_local_dir());
    create_if_nonexistent(&get_config_dir());
    create_if_nonexistent(&get_root_dir().join("logs"));

    chamber_common::Logger::init(
        get_root_dir()
            .join("logs")
            .join("debug.log")
            .to_str()
            .unwrap(),
    );
}

fn is_valid_guid(guid: &str) -> bool {
    if guid.len() != 36 {
        return false;
    }

    if guid.chars().nth(8) != Some('-')
        || guid.chars().nth(13) != Some('-')
        || guid.chars().nth(18) != Some('-')
        || guid.chars().nth(23) != Some('-')
    {
        return false;
    }

    let hex_only: String = guid.chars().filter(|&c| c != '-').collect();

    if hex_only.len() != 32 {
        return false;
    }

    hex_only.chars().all(|c| c.is_ascii_hexdigit())
}

const DB_SETUP_STATEMENTS: &str = r#"
CREATE TABLE IF NOT EXISTS message_types (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

INSERT INTO message_types (name)
SELECT 'system'
WHERE NOT EXISTS (SELECT 1 FROM message_types WHERE name = 'system');

INSERT INTO message_types (name)
SELECT 'user'
WHERE NOT EXISTS (SELECT 1 FROM message_types WHERE name = 'user');

INSERT INTO message_types (name)
SELECT 'assistant'
WHERE NOT EXISTS (SELECT 1 FROM message_types WHERE name = 'assistant');

CREATE TABLE IF NOT EXISTS providers (
    name TEXT PRIMARY KEY
);

INSERT INTO providers (name)
SELECT 'openai'
WHERE NOT EXISTS (SELECT 1 FROM providers WHERE name = 'openai');

INSERT INTO providers (name)
SELECT 'groq'
WHERE NOT EXISTS (SELECT 1 FROM providers WHERE name = 'groq');

INSERT INTO providers (name)
SELECT 'anthropic'
WHERE NOT EXISTS (SELECT 1 FROM providers WHERE name = 'anthropic');

CREATE TABLE IF NOT EXISTS models (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT,
    provider TEXT NOT NULL,
    FOREIGN KEY (provider) REFERENCES providers(name)
);

INSERT INTO models (name, provider)
SELECT 'gpt-4o', 'openai'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'gpt-4o' AND provider = 'openai');

INSERT INTO models (name, provider)
SELECT 'gpt-4o-mini', 'openai'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'gpt-4o-mini' AND provider = 'openai');

INSERT INTO models (name, provider)
SELECT 'o1-preview', 'openai'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'o1-preview' AND provider = 'openai');

INSERT INTO models (name, provider)
SELECT 'o1-mini', 'openai'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'o1-mini' AND provider = 'openai');

INSERT INTO models (name, provider)
SELECT 'llama3-70b-8192', 'groq'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'llama3-70b-8192' AND provider = 'groq');

INSERT INTO models (name, provider)
SELECT 'claude-3-opus-20240229', 'anthropic'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'claude-3-opus-20240229' AND provider = 'anthropic');

INSERT INTO models (name, provider)
SELECT 'claude-3-sonnet-20240229', 'anthropic'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'claude-3-sonnet-20240229' AND provider = 'anthropic');

INSERT INTO models (name, provider)
SELECT 'claude-3-haiku-20240307', 'anthropic'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'claude-3-haiku-20240307' AND provider = 'anthropic');

INSERT INTO models (name, provider)
SELECT 'claude-3-5-sonnet-latest', 'anthropic'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'claude-3-5-sonnet-latest' AND provider = 'anthropic');

INSERT INTO models (name, provider)
SELECT 'claude-3-5-haiku-latest', 'anthropic'
WHERE NOT EXISTS (SELECT 1 FROM models WHERE name = 'claude-3-5-haiku-latest' AND provider = 'anthropic');

CREATE TABLE IF NOT EXISTS conversations (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    message_type_id INTEGER NOT NULL,
    content TEXT NOT NULL,
    api_config_id INTEGER NOT NULL,
    system_prompt TEXT NOT NULL,
    FOREIGN KEY (message_type_id) REFERENCES message_types(id),
    FOREIGN KEY (api_config_id) REFERENCES api_configurations(id)
);

CREATE TABLE IF NOT EXISTS paths (
    id INTEGER PRIMARY KEY,
    conversation_id INTEGER NOT NULL,
    message_id INTEGER NOT NULL,
    sequence INTEGER NOT NULL,
    FOREIGN KEY (conversation_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (message_id) REFERENCES messages(id)
);

CREATE TABLE IF NOT EXISTS forks (
    id INTEGER PRIMARY KEY,
    from_id INTEGER NOT NULL,
    to_id INTEGER NOT NULL,
    FOREIGN KEY (from_id) REFERENCES conversations(id) ON DELETE CASCADE,
    FOREIGN KEY (to_id) REFERENCES conversations(id) ON DELETE CASCADE
);
"#;

// TODO: error handling for the results here
//
// NOTE: this _does not_ create a new message for the response
//       the last message in the conversation is expected to be
//       a placeholder to be filled here for the Assistant
fn completion(
    websocket: &mut tungstenite::WebSocket<std::net::TcpStream>,
    mut payload: Conversation,
    db: &rusqlite::Connection,
) {
    // this needs to be async
    if is_valid_guid(&payload.name) {
        let new_name = network::prompt(
            API::OpenAI(OpenAIModel::GPT4oMini),
            &r#"
                                You will be given the start of a conversation.
                                Give it a name.
                                Guidelines:
                                - No markdown
                                - Respond with _only_ the name.
                                "#
            .to_string(),
            &vec![payload.messages[0].clone()],
        );

        payload.name = new_name
            .unwrap()
            .content
            .chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                c if c.is_alphanumeric() || c == '.' || c == '-' || c == ' ' => c,
                _ => '_',
            })
            .collect();
    }

    // the conversation needs to be set with a db ID at this point
    let mut saved_conversation = payload.clone();
    saved_conversation.upsert(db).unwrap();

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    std::thread::spawn(move || {
        match network::prompt_stream(
            &payload
                .messages
                .iter()
                .map(|m| Message::from(m.clone()))
                .collect(),
            tx,
        ) {
            Ok(_) => {}
            Err(e) => {
                error!("error sending message to GPT endpoint: {}", e);
                std::process::exit(1);
            }
        }
    });

    loop {
        match rx.recv() {
            Ok(message) => {
                let request_id = saved_conversation.messages[saved_conversation.messages.len() - 2]
                    .id
                    .unwrap();

                let last = saved_conversation.messages.last_mut().unwrap();
                last.content.push_str(&message);

                let conversation_id = saved_conversation.id.unwrap();
                let response_id = last.id.unwrap();
                let conversation_name = saved_conversation.name.clone();

                let response = serde_json::to_string(&ArrakisResponse {
                    payload: ResponsePayload::Completion(Completion {
                        stream: true,
                        delta: message,
                        name: conversation_name,
                        conversation_id,
                        request_id,
                        response_id,
                    }),
                })
                .unwrap();

                match websocket.write(tungstenite::Message::text(response)) {
                    Ok(_) => {
                        websocket.flush().unwrap();
                    }
                    Err(e) => {
                        error!("error writing stream to websocket: {}", e);
                        continue;
                    }
                };
            }
            Err(e) => {
                println!("Assuming stream completed... ({})", e);

                let response = serde_json::to_string(&ArrakisResponse {
                    payload: ResponsePayload::CompletionEnd,
                })
                .unwrap();

                match websocket.write(tungstenite::Message::text(response)) {
                    Ok(_) => {
                        websocket.flush().unwrap();
                    }
                    Err(e) => {
                        error!("error writing CompletionEnd to websocket: {}", e);
                        continue;
                    }
                };

                saved_conversation.upsert(db).unwrap();

                break;
            }
        }
    }
}

fn get_conversation(conversation_id: i64, db: &rusqlite::Connection) -> Conversation {
    let mut query = db
        .prepare(
            "
            SELECT
                c.id as conversation_id,
                c.name as conversation_name,
                m.id as message_id,
                m.message_type_id,
                m.content,
                api.provider,
                api.name,
                m.system_prompt,
                l.sequence
            FROM conversations c
            JOIN links l ON c.id = l.conversation_id
            JOIN messages m ON l.message_id = m.id
            JOIN models api ON m.api_config_id = api.id
            WHERE c.id = ?1
            ORDER BY l.sequence ASC
            ",
        )
        .unwrap();

    let rows = query
        .query_map(params![conversation_id], |row| {
            let provider = row.get::<_, String>("provider")?;
            let model_name = row.get::<_, String>("name")?;
            let api = API::from_strings(&provider, &model_name)
                .map_err(|e| rusqlite::Error::InvalidParameterName(e))?;

            Ok((
                row.get::<_, i64>("conversation_id")?,
                row.get::<_, String>("conversation_name")?,
                row.get::<_, i64>("message_id")?,
                MessageType::from_id(row.get::<_, i64>("message_type_id")?).unwrap(),
                row.get::<_, String>("content")?,
                api,
                row.get::<_, String>("system_prompt")?,
                row.get::<_, i32>("sequence")?,
            ))
        })
        .unwrap();

    let mut conversation = Conversation {
        id: Some(conversation_id),
        name: String::new(),
        messages: Vec::new(),
    };

    for row in rows {
        let row = row.unwrap();
        conversation.name = row.1;
        conversation.messages.push(Message {
            id: Some(row.2),
            message_type: row.3,
            content: row.4,
            api: row.5,
            system_prompt: row.6,
            sequence: row.7,
        });
    }

    conversation
}

// TODO: there is zero error handling around here lol

fn main() {
    setup();

    let db_ = std::sync::Arc::new(std::sync::Mutex::new(
        rusqlite::Connection::open(get_local_dir().join("william.sqlite"))
            .expect("Failed to open database"),
    ));

    db_.lock()
        .unwrap()
        .execute_batch(DB_SETUP_STATEMENTS)
        .expect("Failed to initialize database");

    let dewey_ = std::sync::Arc::new(std::sync::Mutex::new(dewey_lib::Dewey::new().unwrap()));

    return;

    let server = std::net::TcpListener::bind("127.0.0.1:9001").unwrap();
    println!("WebSocket server listening on ws://127.0.0.1:9001");

    for stream in server.incoming() {
        let db = std::sync::Arc::clone(&db_);
        std::thread::spawn(move || {
            let stream = stream.unwrap();
            let mut websocket = tungstenite::accept(stream).unwrap();

            loop {
                let msg = match websocket.read() {
                    Ok(m) => m,
                    Err(e) => {
                        error!("error reading from websocket: {}", e);
                        continue;
                    }
                };

                let request: ArrakisRequest = match msg {
                    tungstenite::Message::Close(_) => {
                        break;
                    }
                    tungstenite::Message::Text(t) => match serde_json::from_str(&t) {
                        Ok(r) => r,
                        Err(e) => {
                            error!("t: {}", t);
                            error!("error reading Arrakis request: {}", e);
                            continue;
                        }
                    },
                    _ => {
                        error!("unsupported message type");
                        continue;
                    }
                };

                match request {
                    ArrakisRequest::Completion { payload } => {
                        completion(&mut websocket, payload, &db.lock().unwrap());
                    }
                    ArrakisRequest::Ping { payload: _ } => {
                        let response = serde_json::to_string(&ArrakisResponse {
                            payload: ResponsePayload::Ping(Ping {
                                body: "pong".to_string(),
                            }),
                        })
                        .unwrap();

                        match websocket.write(tungstenite::Message::text(response)) {
                            Ok(_) => {
                                websocket.flush().unwrap();
                            }
                            Err(e) => {
                                error!("error writing to websocket: {}", e);
                                continue;
                            }
                        };
                    }
                    ArrakisRequest::ConversationList => {
                        let db = db.lock().unwrap();
                        let mut query = db.prepare("SELECT id, name from conversations").unwrap();
                        let conversations = query
                            .query_map(params![], |row| {
                                Ok(Conversation {
                                    id: row.get(0)?,
                                    name: row.get(1)?,
                                    messages: Vec::new(),
                                })
                            })
                            .unwrap()
                            .map(|c| c.unwrap())
                            .collect();

                        let response = serde_json::to_string(&ArrakisResponse {
                            payload: ResponsePayload::ConversationList(ConversationList {
                                conversations,
                            }),
                        })
                        .unwrap();

                        match websocket.write(tungstenite::Message::text(response)) {
                            Ok(_) => {
                                websocket.flush().unwrap();
                            }
                            Err(e) => {
                                error!("error writing to websocket: {}", e);
                                continue;
                            }
                        };
                    }
                    ArrakisRequest::Load { payload } => {
                        let response = serde_json::to_string(&ArrakisResponse {
                            payload: ResponsePayload::Load(
                                get_conversation(payload.id, &db.lock().unwrap()).into(),
                            ),
                        })
                        .unwrap();

                        match websocket.write(tungstenite::Message::text(response)) {
                            Ok(_) => {
                                websocket.flush().unwrap();
                            }
                            Err(e) => {
                                error!("error writing to websocket: {}", e);
                                continue;
                            }
                        };
                    }
                    ArrakisRequest::SystemPrompt { payload } => {
                        let path = get_config_dir().join("system_prompt");

                        if payload.write {
                            match std::fs::write(path.clone(), payload.content) {
                                Ok(_) => {
                                    println!("system prompt saved to {}", path.to_str().unwrap());
                                }
                                Err(e) => {
                                    error!("error saving conversation: {}", e);
                                }
                            };

                            continue;
                        }

                        let content = match std::fs::read_to_string(path.clone()) {
                            Ok(c) => c,
                            Err(e) => {
                                lprint!(
                                    error,
                                    "error reading system prompt file {}: {}",
                                    path.to_str().unwrap(),
                                    e
                                );
                                continue;
                            }
                        };

                        let response = serde_json::to_string(&ArrakisResponse {
                            payload: ResponsePayload::SystemPrompt(SystemPrompt {
                                write: false,
                                content,
                            }),
                        })
                        .unwrap();

                        match websocket.write(tungstenite::Message::text(response)) {
                            Ok(_) => {
                                websocket.flush().unwrap();
                            }
                            Err(e) => {
                                error!("error writing to websocket: {}", e);
                                continue;
                            }
                        };
                    }
                    // get the current conversation,
                    // create the fork,
                    // carry on with the completion
                    ArrakisRequest::Fork { payload } => {
                        let db = db.lock().unwrap();

                        let mut conversation = get_conversation(payload.conversation_id, &db);

                        conversation.id = None;
                        conversation.name = format!("Fork: {}", conversation.name);
                        conversation.messages = conversation
                            .messages
                            .iter()
                            .take(payload.sequence as usize)
                            .cloned()
                            .collect();

                        let mut assistant_message = conversation.messages.last().unwrap().clone();

                        if assistant_message.message_type != MessageType::Assistant {
                            assistant_message.id = None;
                            assistant_message.message_type = MessageType::Assistant;
                            assistant_message.content = String::new();
                            assistant_message.sequence += 1;

                            conversation.messages.push(assistant_message);
                        } else {
                            let last = conversation.messages.last_mut().unwrap();
                            last.content = String::new();
                        }

                        let _ = conversation.upsert(&db);
                        let new_id = db.last_insert_rowid();

                        let fork_query = "INSERT INTO forks (from_id, to_id) VALUES (?, ?)";
                        db.execute(fork_query, params![payload.conversation_id, new_id])
                            .unwrap();

                        completion(&mut websocket, conversation, &db);
                    }
                };
            }
        });
    }
}
