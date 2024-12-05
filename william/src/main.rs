use rusqlite::params;

use chamber_common::{error, lprint, Logger};

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

fn get_home_dir() -> std::path::PathBuf {
    match std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(|_| {
            std::env::var("HOMEDRIVE").and_then(|homedrive| {
                std::env::var("HOMEPATH").map(|homepath| format!("{}{}", homedrive, homepath))
            })
        }) {
        Ok(dir) => std::path::PathBuf::from(dir),
        Err(_) => panic!("Failed to get home directory"),
    }
}

fn get_local_dir() -> std::path::PathBuf {
    let home_dir = get_home_dir();
    home_dir.join(".local/william")
}

fn get_config_dir() -> std::path::PathBuf {
    let home_dir = get_home_dir();
    home_dir.join(".config/william")
}

fn get_conversations_dir() -> std::path::PathBuf {
    let local = get_local_dir();
    local.join("conversations")
}

// TODO: a lot of this setup code needs abstracted to a common module
//       with it being something like
//       ~/.<local|config>/chamber/...
fn setup() {
    let local_path = get_local_dir();
    let config_path = get_config_dir();

    let conversations_path = get_conversations_dir();
    let logging_path = local_path.join("logs");

    create_if_nonexistent(&local_path);
    create_if_nonexistent(&config_path);
    create_if_nonexistent(&conversations_path);
    create_if_nonexistent(&logging_path);

    chamber_common::Logger::init(logging_path.join("debug.log").to_str().unwrap().to_string());
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

fn main() {
    setup();

    let db_ = std::sync::Arc::new(std::sync::Mutex::new(
        rusqlite::Connection::open(get_local_dir().join("william.sqlite"))
            .expect("Failed to open database"),
    ));

    db_.lock()
        .unwrap()
        .execute_batch(&std::fs::read_to_string("db.sql").unwrap())
        .expect("Failed to initialize database");

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
                    ArrakisRequest::Completion { mut payload } => {
                        // this needs to be async
                        if false
                        /*is_valid_guid(&payload.name)*/
                        {
                            let new_name = network::prompt(
                                &"openai".to_string(),
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
                                    c if c.is_alphanumeric()
                                        || c == '.'
                                        || c == '-'
                                        || c == ' ' =>
                                    {
                                        c
                                    }
                                    _ => '_',
                                })
                                .collect();
                        }

                        // the conversation needs to be set with a db ID at this point
                        let mut saved_conversation = payload.clone();
                        saved_conversation.upsert(&db.lock().unwrap()).unwrap();

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
                                    let request_id = saved_conversation.messages
                                        [saved_conversation.messages.len() - 2]
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
                                            error!(
                                                "error writing CompletionEnd to websocket: {}",
                                                e
                                            );
                                            continue;
                                        }
                                    };

                                    saved_conversation.upsert(&db.lock().unwrap()).unwrap();

                                    break;
                                }
                            }
                        }
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
                        let db = db.lock().unwrap();
                        let mut query = db
                            .prepare(
                                "
                                    SELECT
                                        c.id as conversation_id,
                                        c.name as conversation_name,
                                        m.id as message_id,
                                        m.message_type_id,
                                        m.content,
                                        m.model,
                                        m.system_prompt,
                                        l.sequence
                                    FROM conversations c
                                    JOIN links l ON c.id = l.conversation_id
                                    JOIN messages m ON l.message_id = m.id
                                    WHERE c.id = ?1
                                    ORDER BY l.sequence ASC
                                ",
                            )
                            .unwrap();

                        let rows = query
                            .query_map(params![payload.id], |row| {
                                Ok((
                                    row.get::<_, i64>("conversation_id")?,
                                    row.get::<_, String>("conversation_name")?,
                                    row.get::<_, i64>("message_id")?,
                                    MessageType::from_id(row.get::<_, i64>("message_type_id")?)
                                        .unwrap(),
                                    row.get::<_, String>("content")?,
                                    row.get::<_, String>("model")?,
                                    row.get::<_, String>("system_prompt")?,
                                    row.get::<_, i32>("sequence")?,
                                ))
                            })
                            .unwrap();

                        let mut conversation = Conversation {
                            id: Some(payload.id),
                            name: String::new(),
                            messages: Vec::new(),
                        };

                        for row in rows {
                            let row = row.unwrap();
                            println!("row: {:?}", row);
                            conversation.name = row.1;
                            conversation.messages.push(Message {
                                id: Some(row.2),
                                message_type: row.3,
                                content: row.4,
                                model: row.5,
                                system_prompt: row.6,
                                sequence: row.7,
                            });
                        }

                        let response = serde_json::to_string(&ArrakisResponse {
                            payload: ResponsePayload::Load(conversation.into()),
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
                };
            }
        });
    }
}
