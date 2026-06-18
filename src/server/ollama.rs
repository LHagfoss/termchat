use colored::Colorize;
use std::sync::Arc;

use crate::protocol::ServerToClient;
use crate::server::state::ServerState;
use crate::server_log;

pub async fn handle_ask(question: String, username: String, state: Arc<ServerState>) {
    let ollama_api = std::env::var("OLLAMA_API_URL")
        .unwrap_or_else(|_| "http://192.168.1.254:11434/v1".to_string());
    let client = reqwest::Client::new();

    let models_url = format!("{}/models", ollama_api.trim_end_matches('/'));
    let model = match client.get(&models_url).send().await {
        Ok(resp) => {
            #[derive(serde::Deserialize)]
            struct ModelData {
                id: String,
            }
            #[derive(serde::Deserialize)]
            struct ModelsResponse {
                data: Vec<ModelData>,
            }
            if let Ok(models) = resp.json::<ModelsResponse>().await {
                models
                    .data
                    .first()
                    .map(|m| m.id.clone())
                    .unwrap_or_else(|| "llama3".to_string())
            } else {
                "llama3".to_string()
            }
        }
        Err(_) => "llama3".to_string(),
    };

    let active_users_list = {
        let u = state.users.lock().await;
        u.iter().cloned().collect::<Vec<String>>().join(", ")
    };
    let chat_history_text = {
        let h = state.history.lock().await;
        h.iter().cloned().collect::<Vec<String>>().join("\n")
    };

    let system_prompt = format!(
        "You are a helpful chat assistant named Ollama inside a terminal chatroom.\n\
         Room context:\n\
         - Server Name: {}\n\
         - Current Online Users: [{}]\n\
         - User asking you: {}\n\n\
         Recent Chat History (for context):\n\
         ---\n\
         {}\n\
         ---\n\n\
         Instructions:\n\
         - Keep your response brief, max 200 characters or words.\n\
         - Do not output any <think> tags or internal thinking steps; respond directly and concisely.",
        state.server_name, active_users_list, username, chat_history_text
    );

    #[derive(serde::Serialize)]
    struct Message {
        role: String,
        content: String,
    }
    #[derive(serde::Serialize)]
    struct ChatRequest {
        model: String,
        messages: Vec<Message>,
    }

    let completions_url = format!("{}/chat/completions", ollama_api.trim_end_matches('/'));
    let req_body = ChatRequest {
        model: model.clone(),
        messages: vec![
            Message {
                role: "system".to_string(),
                content: system_prompt,
            },
            Message {
                role: "user".to_string(),
                content: question,
            },
        ],
    };

    match client.post(&completions_url).json(&req_body).send().await {
        Ok(resp) => {
            #[derive(serde::Deserialize)]
            struct ChatChoice {
                message: MessageContent,
            }
            #[derive(serde::Deserialize)]
            struct MessageContent {
                content: String,
            }
            #[derive(serde::Deserialize)]
            struct ChatResponse {
                choices: Vec<ChatChoice>,
            }

            if let Ok(chat_resp) = resp.json::<ChatResponse>().await {
                if let Some(choice) = chat_resp.choices.first() {
                    let reply = choice.message.content.trim();
                    server_log!(Info, "Ollama successfully answered to '{}' using model '{}'", username, model);

                    {
                        let mut hist = state.history.lock().await;
                        if hist.len() >= 20 {
                            hist.pop_front();
                        }
                        hist.push_back(format!("Ollama: {}", reply));
                    }

                    let response_content = reply.to_string();
                    let _ = state.tx.send(ServerToClient::Broadcast {
                        sender: "Ollama".to_string(),
                        content: response_content,
                        timestamp: chrono::Utc::now(),
                        sender_color: Some("cyan".to_string()),
                    });
                } else {
                    server_log!(Warn, "Ollama query for '{}' returned empty choices using model '{}'", username, model);
                    let _ = state.tx.send(ServerToClient::Broadcast {
                        sender: "Ollama".to_string(),
                        content: "Error: No completion choices returned from model.".to_string(),
                        timestamp: chrono::Utc::now(),
                        sender_color: Some("cyan".to_string()),
                    });
                }
            } else {
                server_log!(Error, "Ollama query for '{}' failed to parse response using model '{}'", username, model);
                let _ = state.tx.send(ServerToClient::Broadcast {
                    sender: "Ollama".to_string(),
                    content: "Error: Failed to parse response from Ollama API.".to_string(),
                    timestamp: chrono::Utc::now(),
                    sender_color: Some("cyan".to_string()),
                });
            }
        }
        Err(e) => {
            server_log!(Error, "Ollama query for '{}' failed to connect using model '{}': {}", username, model, e);
            let _ = state.tx.send(ServerToClient::Broadcast {
                sender: "Ollama".to_string(),
                content: format!("Error: Failed to connect to Ollama: {}", e),
                timestamp: chrono::Utc::now(),
                sender_color: Some("cyan".to_string()),
            });
        }
    }
}
