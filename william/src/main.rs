use chamber_common::{error, lprint, Logger};

use crate::types::{
    ArrakisRequest, ArrakisResponse, Completion, Conversation, ConversationList, Ping,
    RequestPayload, ResponsePayload, SystemPrompt,
};

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
    let home_dir = get_home_dir();

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

fn main() {
    setup();
    let server = std::net::TcpListener::bind("127.0.0.1:9001").unwrap();
    println!("WebSocket server listening on ws://127.0.0.1:9001");

    for stream in server.incoming() {
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

                match request.payload {
                    RequestPayload::Completion(payload) => {
                        let (tx, rx) = std::sync::mpsc::channel::<String>();
                        let mut saved_conversation = payload.clone();
                        std::thread::spawn(move || {
                            match network::prompt_stream(&payload.conversation, tx) {
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
                                    let last = saved_conversation.conversation.last_mut().unwrap();
                                    last.content.push_str(&message);

                                    let response = serde_json::to_string(&ArrakisResponse {
                                        payload: ResponsePayload::Completion(Completion {
                                            stream: true,
                                            delta: message,
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

                                    // save the conversation
                                    let conversations_dir = get_conversations_dir();
                                    let contents = match serde_json::to_string(&saved_conversation)
                                    {
                                        Ok(c) => c,
                                        Err(e) => {
                                            error!("error serializing conversation: {}", e);
                                            String::new()
                                        }
                                    };

                                    let filename = conversations_dir
                                        .join(format!("{}.json", saved_conversation.name));
                                    match std::fs::write(filename.clone(), contents) {
                                        Ok(_) => {
                                            println!(
                                                "conversation saved to {}",
                                                filename.to_str().unwrap()
                                            );
                                        }
                                        Err(e) => {
                                            error!("error saving conversation: {}", e);
                                        }
                                    };

                                    break;
                                }
                            }
                        }
                    }
                    RequestPayload::Ping(_) => {
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
                    RequestPayload::ConversationList => {
                        let mut conversations = Vec::new();
                        for entry in std::fs::read_dir(get_conversations_dir()).unwrap() {
                            let entry = entry.unwrap();
                            conversations.push(entry.file_name().into_string().unwrap());
                        }

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
                    RequestPayload::Load(payload) => {
                        let path = get_conversations_dir().join(format!("{}", payload.name));
                        let contents = match std::fs::read_to_string(path.clone()) {
                            Ok(c) => c,
                            Err(e) => {
                                lprint!(
                                    error,
                                    "error reading conversation file {}: {}",
                                    path.to_str().unwrap(),
                                    e
                                );
                                continue;
                            }
                        };

                        let conversation: Conversation = serde_json::from_str(&contents).unwrap();
                        let response = serde_json::to_string(&ArrakisResponse {
                            payload: ResponsePayload::Load(conversation),
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
                    RequestPayload::SystemPrompt(payload) => {
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
