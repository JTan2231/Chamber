use std::env;
use std::io::BufRead;

use chamber_common::{error, info, Logger};

use crate::types::*;

// TODO: there needs to be some refactoring done here
//       to accommodate the fact that model/system prompt metadata
//       is bundled with the messages

fn build_request(
    client: &reqwest::blocking::Client,
    params: &RequestParams,
) -> reqwest::blocking::RequestBuilder {
    let body = match params.provider.as_str() {
        "openai" => serde_json::json!({
            "model": params.model,
            "messages": params.messages.iter()
                .map(|message| {
                    serde_json::json!({
                        "role": message.message_type.to_string(),
                        "content": message.content
                    })
                }).collect::<Vec<serde_json::Value>>(),
            "stream": params.stream,
        }),
        "groq" => serde_json::json!({
            "model": params.model,
            "messages": params.messages.iter()
                .map(|message| {
                    serde_json::json!({
                        "role": message.message_type.to_string(),
                        "content": message.content
                    })
                }).collect::<Vec<serde_json::Value>>(),
            "stream": params.stream,
        }),
        "anthropic" => serde_json::json!({
            "model": params.model,
            "messages": params.messages.iter().map(|message| {
                serde_json::json!({
                    "role": message.message_type.to_string(),
                    "content": message.content
                })
            }).collect::<Vec<serde_json::Value>>(),
            "stream": params.stream,
            "max_tokens": params.max_tokens.unwrap(),
            "system": params.system_prompt.clone().unwrap(),
        }),
        "gemini" => serde_json::json!({
            "contents": params.messages.iter().map(|m| {
                serde_json::json!({
                    "parts": [{
                        "text": m.content
                    }],
                    "role": match m.message_type {
                        MessageType::User => "user",
                        MessageType::Assistant => "model",
                        _ => panic!("what is happening")
                    }
                })
            }).collect::<Vec<_>>(),
            "systemInstruction": {
                "parts": [{
                    "text": params.system_prompt,
                }]
            }
        }),
        _ => panic!("Invalid provider for request_body: {}", params.provider),
    };

    let url = format!("https://{}:{}{}", params.host, params.port, params.path);
    let mut request = client.post(url.clone()).json(&body);

    match params.provider.as_str() {
        "openai" | "groq" => {
            request = request.header(
                "Authorization",
                format!("Bearer {}", params.authorization_token),
            );
        }
        "anthropic" => {
            request = request
                .header("x-api-key", &params.authorization_token)
                .header("anthropic-version", "2023-06-01");
        }
        "gemini" => {
            request = client
                .post(format!("{}?key={}", url, params.authorization_token))
                .json(&body);
        }
        _ => panic!("Invalid provider: {}", params.provider),
    }

    request
}

fn get_openai_request_params(
    system_prompt: String,
    api: API,
    chat_history: &Vec<Message>,
    stream: bool,
) -> RequestParams {
    let (provider, model) = api.to_strings();
    RequestParams {
        provider,
        host: "api.openai.com".to_string(),
        path: "/v1/chat/completions".to_string(),
        port: 443,
        messages: if model.contains("o1") {
            vec![]
        } else {
            vec![Message {
                id: None,
                message_type: MessageType::Developer,
                content: system_prompt.clone(),
                api,
                system_prompt,
                sequence: -1,
                date_created: String::new(),
            }]
        }
        .iter()
        .chain(chat_history.iter())
        .cloned()
        .collect::<Vec<Message>>(),
        model,
        stream,
        authorization_token: env::var("OPENAI_API_KEY")
            .expect("OPENAI_API_KEY environment variable not set"),
        max_tokens: None,
        system_prompt: None,
    }
}

// this is basically a copy of the openai_request_params
fn get_groq_request_params(
    system_prompt: String,
    api: API,
    chat_history: &Vec<Message>,
    stream: bool,
) -> RequestParams {
    let (provider, model) = api.to_strings();
    RequestParams {
        provider,
        host: "api.groq.com".to_string(),
        path: "/openai/v1/chat/completions".to_string(),
        port: 443,
        messages: vec![Message {
            id: None,
            message_type: MessageType::System,
            content: system_prompt.clone(),
            api: API::Groq(GroqModel::LLaMA70B),
            system_prompt,
            sequence: -1,
            date_created: String::new(),
        }]
        .iter()
        .chain(chat_history.iter())
        .cloned()
        .collect::<Vec<Message>>(),
        model,
        stream,
        authorization_token: env::var("GROQ_API_KEY")
            .expect("GRQO_API_KEY environment variable not set"),
        max_tokens: None,
        system_prompt: None,
    }
}

fn get_anthropic_request_params(
    system_prompt: String,
    api: API,
    chat_history: &Vec<Message>,
    stream: bool,
) -> RequestParams {
    let (provider, model) = api.to_strings();
    RequestParams {
        provider,
        host: "api.anthropic.com".to_string(),
        path: "/v1/messages".to_string(),
        port: 443,
        messages: chat_history.iter().cloned().collect::<Vec<Message>>(),
        model,
        stream,
        authorization_token: env::var("ANTHROPIC_API_KEY")
            .expect("ANTHROPIC_API_KEY environment variable not set"),
        max_tokens: Some(4096),
        system_prompt: Some(system_prompt),
    }
}

// TODO: model enums + etc.
fn get_gemini_request_params(
    system_prompt: String,
    api: API,
    chat_history: &Vec<Message>,
    stream: bool,
) -> RequestParams {
    let (provider, model) = api.to_strings();
    RequestParams {
        provider,
        host: "generativelanguage.googleapis.com".to_string(),
        path: "/v1beta/models/gemini-1.5-flash-latest:generateContent".to_string(),
        port: 443,
        messages: chat_history.iter().cloned().collect::<Vec<Message>>(),
        model,
        stream,
        authorization_token: env::var("GEMINI_API_KEY")
            .expect("GEMINI_API_KEY environment variable not set"),
        max_tokens: Some(4096),
        system_prompt: Some(system_prompt),
    }
}

fn get_params(
    system_prompt: &str,
    api: API,
    chat_history: &Vec<Message>,
    stream: bool,
) -> RequestParams {
    match api {
        API::Anthropic(_) => get_anthropic_request_params(
            system_prompt.to_string(),
            api.clone(),
            chat_history,
            stream,
        ),
        API::OpenAI(_) => {
            get_openai_request_params(system_prompt.to_string(), api.clone(), chat_history, stream)
        }
        API::Groq(_) => {
            get_groq_request_params(system_prompt.to_string(), api.clone(), chat_history, stream)
        }
    }
}

fn send_delta(tx: &std::sync::mpsc::Sender<String>, delta: String) {
    match tx.send(delta.clone()) {
        Ok(_) => {}
        Err(e) => {
            error!("error sending transmission error string: {}", e);
        }
    };
}

fn unescape(content: &str) -> String {
    content
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace("\\\"", "\"")
        .replace("\\'", "'")
        .replace("\\\\", "\\")
}

// TODO: at some point i think the tokenizer will have to come down here
//       as that's how we'll track usage metrics from streams

fn process_openai_stream(
    response: reqwest::blocking::Response,
    tx: &std::sync::mpsc::Sender<String>,
) -> Result<String, std::io::Error> {
    info!("processing openai stream");
    let reader = std::io::BufReader::new(response);
    let mut full_message = String::new();

    for line in reader.lines() {
        let line = line?;
        if !line.starts_with("data: ") {
            continue;
        }

        let payload = line[6..].trim();
        if payload.is_empty() || payload == "[DONE]" {
            break;
        }

        let response_json: serde_json::Value = match serde_json::from_str(&payload) {
            Ok(json) => json,
            Err(e) => {
                error!("JSON parse error: {}", e);
                error!("Error payload: {}", payload);
                continue;
            }
        };

        let mut delta = unescape(&response_json["choices"][0]["delta"]["content"].to_string());
        if delta != "null" {
            delta = delta[1..delta.len() - 1].to_string();
            send_delta(tx, delta.clone());

            full_message.push_str(&delta);
        }
    }

    // TODO: actually calculate the usage, obviously
    Ok(full_message)
}

fn process_anthropic_stream(
    response: reqwest::blocking::Response,
    tx: &std::sync::mpsc::Sender<String>,
) -> Result<String, std::io::Error> {
    info!("processing anthropic stream");
    let reader = std::io::BufReader::new(response);
    let mut full_message = String::new();

    for line in reader.lines() {
        let line = line?;

        if line.starts_with("event: message_stop") {
            break;
        }

        if !line.starts_with("data: ") {
            continue;
        }

        let payload = line[6..].trim();
        if payload.is_empty() || payload == "[DONE]" {
            break;
        }

        let response_json: serde_json::Value = match serde_json::from_str(&payload) {
            Ok(json) => json,
            Err(e) => {
                error!("JSON parse error: {}", e);
                error!("Error payload: {}", payload);
                continue;
            }
        };

        let mut delta = "null".to_string();
        if response_json["type"] == "content_block_delta" {
            delta = unescape(&response_json["delta"]["text"].to_string());
            // Trim quotes from delta
            delta = delta[1..delta.len() - 1].to_string();
        }

        if delta != "null" {
            send_delta(tx, delta.clone());
            full_message.push_str(&delta);
        }
    }

    Ok(full_message)
}

// TODO: error handling
//
/// JSON response handler for `prompt`
/// Ideally I think there should be more done here,
/// maybe something like getting usage metrics out of this
fn read_json_response(api: &API, response_json: &serde_json::Value) -> String {
    match api {
        API::Anthropic(_) => response_json["choices"][0]["message"]["content"].to_string(),
        API::OpenAI(_) => response_json["choices"][0]["message"]["content"].to_string(),
        API::Groq(_) => response_json["content"][0]["text"].to_string(),
        // TODO: gemini
        //_ => response_json["candidates"][0]["content"]["parts"][0]["text"].to_string(),
    }
}

// TODO: I'm wondering if it's even worth making a synchronous version
//
/// Function for streaming responses from the LLM.
/// Asynchronous by default--relies on message channels.
pub fn prompt_stream(
    api: API,
    chat_history: &Vec<Message>,
    system_prompt: &str,
    tx: std::sync::mpsc::Sender<String>,
) -> Result<Message, std::io::Error> {
    let params = get_params(system_prompt, api.clone(), chat_history, true);
    let client = reqwest::blocking::Client::new();

    let request = build_request(&client, &params);
    let response = build_request(&client, &params)
        .send()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_body = response
            .text()
            .unwrap_or_else(|_| String::from("Could not read error response"));

        return Err(std::io::Error::new(std::io::ErrorKind::Other, error_body));
    }

    let content = match api {
        API::Anthropic(_) => process_anthropic_stream(response, &tx),
        API::OpenAI(_) => process_openai_stream(response, &tx),
        API::Groq(_) => process_openai_stream(response, &tx),
    }?;

    Ok(Message {
        id: None,
        message_type: MessageType::Assistant,
        content,
        api,
        system_prompt: system_prompt.to_string(),
        sequence: -1,
        date_created: String::new(),
    })
}

/// Ad-hoc prompting for an LLM
/// Makes zero expectations about the state of the conversation
/// and returns a tuple of (response message, usage from the prompt)
pub fn prompt(
    api: API,
    system_prompt: &str,
    chat_history: &Vec<Message>,
) -> Result<Message, Box<dyn std::error::Error>> {
    let params = get_params(system_prompt, api.clone(), chat_history, false);
    let client = reqwest::blocking::Client::new();

    let response = build_request(&client, &params).send()?;
    let response_json: serde_json::Value = response.json()?;

    let mut content = read_json_response(&api, &response_json);

    content = unescape(&content);
    if content.starts_with("\"") && content.ends_with("\"") {
        content = content[1..content.len() - 1].to_string();
    }

    Ok(Message {
        id: None,
        message_type: MessageType::Assistant,
        content,
        api,
        system_prompt: system_prompt.to_string(),
        sequence: -1,
        date_created: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    fn setup_test_env() {
        env::set_var("GROQ_API_KEY", "test_groq_key");
        env::set_var("OPENAI_API_KEY", "test_openai_key");
        env::set_var("ANTHROPIC_API_KEY", "test_anthropic_key");
    }

    fn create_test_message(message_type: MessageType, content: &str, api: API) -> Message {
        Message {
            id: None,
            message_type,
            content: content.to_string(),
            api,
            system_prompt: "".to_string(),
            sequence: -1,
            date_created: String::new(),
        }
    }

    #[test]
    fn test_groq_basic_params() {
        setup_test_env();
        let system_prompt = "test system prompt".to_string();
        let api = API::Groq(GroqModel::LLaMA70B);
        let chat_history = vec![create_test_message(MessageType::User, "Hello", api.clone())];

        let params = get_groq_request_params(system_prompt.clone(), api, &chat_history, false);

        assert_eq!(params.provider, "groq");
        assert_eq!(params.host, "api.groq.com");
        assert_eq!(params.path, "/openai/v1/chat/completions");
        assert_eq!(params.max_tokens, None);
        assert_eq!(params.system_prompt, None);
    }

    #[test]
    fn test_openai_basic_params() {
        setup_test_env();
        let system_prompt = "test system prompt".to_string();
        let api = API::OpenAI(OpenAIModel::GPT4o);
        let chat_history = vec![create_test_message(MessageType::User, "Hello", api.clone())];

        let params = get_openai_request_params(system_prompt.clone(), api, &chat_history, false);

        assert_eq!(params.provider, "openai");
        assert_eq!(params.host, "api.openai.com");
        assert_eq!(params.path, "/v1/chat/completions");
        assert_eq!(params.max_tokens, None);
        assert_eq!(params.system_prompt, None);
    }

    #[test]
    fn test_anthropic_basic_params() {
        setup_test_env();
        let system_prompt = "test system prompt".to_string();
        let api = API::Anthropic(AnthropicModel::Claude35Sonnet);
        let chat_history = vec![create_test_message(MessageType::User, "Hello", api.clone())];

        let params = get_anthropic_request_params(system_prompt.clone(), api, &chat_history, false);

        assert_eq!(params.provider, "anthropic");
        assert_eq!(params.host, "api.anthropic.com");
        assert_eq!(params.path, "/v1/messages");
        assert_eq!(params.max_tokens, Some(4096));
        assert_eq!(params.system_prompt, Some(system_prompt));
    }

    #[test]
    fn test_message_handling() {
        setup_test_env();
        let system_prompt = "test prompt".to_string();
        let chat_history = vec![
            Message {
                id: None,
                message_type: MessageType::User,
                content: "First".to_string(),
                api: API::OpenAI(OpenAIModel::GPT4o),
                system_prompt: "".to_string(),
                sequence: -1,
                date_created: String::new(),
            },
            Message {
                id: None,
                message_type: MessageType::Assistant,
                content: "Second".to_string(),
                api: API::OpenAI(OpenAIModel::GPT4o),
                system_prompt: "".to_string(),
                sequence: -1,
                date_created: String::new(),
            },
        ];

        let providers = vec![
            (API::Groq(GroqModel::LLaMA70B), "groq"),
            (API::OpenAI(OpenAIModel::GPT4o), "openai"),
            (API::Anthropic(AnthropicModel::Claude35Sonnet), "anthropic"),
        ];

        for (api, provider_name) in providers {
            let params = match api.clone() {
                API::Groq(_) => {
                    get_groq_request_params(system_prompt.clone(), api, &chat_history, false)
                }
                API::OpenAI(_) => {
                    get_openai_request_params(system_prompt.clone(), api, &chat_history, false)
                }
                API::Anthropic(_) => {
                    get_anthropic_request_params(system_prompt.clone(), api, &chat_history, false)
                }
            };

            match provider_name {
                "gemini" | "anthropic" => {
                    assert_eq!(
                        params.messages.len(),
                        2,
                        "Wrong message count for {}",
                        provider_name
                    );
                }
                "groq" | "openai" => {
                    assert_eq!(
                        params.messages.len(),
                        3,
                        "Wrong message count for {}",
                        provider_name
                    );
                    assert_eq!(params.messages[0].message_type, MessageType::System);
                }
                _ => unreachable!(),
            }
        }
    }

    #[test]
    fn test_api_key_handling() {
        let test_cases = vec![
            ("GROQ_API_KEY", API::Groq(GroqModel::LLaMA70B)),
            ("OPENAI_API_KEY", API::OpenAI(OpenAIModel::GPT4o)),
            (
                "ANTHROPIC_API_KEY",
                API::Anthropic(AnthropicModel::Claude35Sonnet),
            ),
        ];

        for (key, api) in test_cases {
            env::remove_var(key);
            let system_prompt = "test".to_string();
            let chat_history = vec![];
            let result = std::panic::catch_unwind(|| match api {
                API::Groq(_) => get_groq_request_params(
                    system_prompt.clone(),
                    api.clone(),
                    &chat_history,
                    false,
                ),
                API::OpenAI(_) => get_openai_request_params(
                    system_prompt.clone(),
                    api.clone(),
                    &chat_history,
                    false,
                ),
                API::Anthropic(_) => get_anthropic_request_params(
                    system_prompt.clone(),
                    api.clone(),
                    &chat_history,
                    false,
                ),
            });
            assert!(result.is_err(), "Should panic when {} is not set", key);
        }
    }

    #[test]
    fn test_streaming_all_providers() {
        setup_test_env();
        let system_prompt = "test".to_string();
        let chat_history = vec![];

        let providers = vec![
            API::Groq(GroqModel::LLaMA70B),
            API::OpenAI(OpenAIModel::GPT4o),
            API::Anthropic(AnthropicModel::Claude35Sonnet),
        ];

        for api in providers {
            let params = match api.clone() {
                API::Groq(_) => {
                    get_groq_request_params(system_prompt.clone(), api, &chat_history, true)
                }
                API::OpenAI(_) => {
                    get_openai_request_params(system_prompt.clone(), api, &chat_history, true)
                }
                API::Anthropic(_) => {
                    get_anthropic_request_params(system_prompt.clone(), api, &chat_history, true)
                }
            };
            assert!(params.stream);
        }
    }
}
