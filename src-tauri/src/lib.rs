use reqwest::Client;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct OllamaStatus {
    base_url: String,
    version: String,
    models: Vec<OllamaModel>,
    running_models: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaModel {
    name: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    modified_at: String,
    #[serde(default)]
    details: Option<ModelDetails>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ModelDetails {
    family: Option<String>,
    parameter_size: Option<String>,
    quantization_level: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<OllamaModel>,
}

#[derive(Debug, Deserialize)]
struct RunningResponse {
    #[serde(default)]
    models: Vec<RunningModel>,
}

#[derive(Debug, Deserialize)]
struct RunningModel {
    name: Option<String>,
    model: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VersionResponse {
    version: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ChatMessage {
    id: String,
    role: String,
    content: String,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    metrics: Option<ChatMetrics>,
}

#[derive(Debug, Serialize, Deserialize)]
struct OllamaMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ProviderMessage {
    role: String,
    content: String,
}

fn provider_key_entry(provider: &str) -> Result<keyring::Entry, String> {
    if !matches!(provider, "gemini" | "openai" | "anthropic") {
        return Err("Choose Gemini, OpenAI, or Anthropic.".to_string());
    }
    keyring::Entry::new("ContextCraft", &format!("provider-{provider}"))
        .map_err(|error| format!("Could not access Windows Credential Manager: {error}"))
}

#[tauri::command]
fn save_provider_key(provider: String, api_key: String) -> Result<(), String> {
    let key = api_key.trim();
    if key.len() < 8 || key.len() > 4096 {
        return Err("Enter a valid API key.".to_string());
    }
    provider_key_entry(&provider)?
        .set_password(key)
        .map_err(|error| format!("Could not save the key to Windows Credential Manager: {error}"))
}

#[tauri::command]
fn has_provider_key(provider: String) -> Result<bool, String> {
    match provider_key_entry(&provider)?.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(error) => Err(format!("Could not check Windows Credential Manager: {error}")),
    }
}

#[tauri::command]
fn delete_provider_key(provider: String) -> Result<(), String> {
    match provider_key_entry(&provider)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(format!("Could not remove the key from Windows Credential Manager: {error}")),
    }
}

#[tauri::command]
async fn list_provider_models(provider: String) -> Result<Vec<String>, String> {
    let api_key = provider_key_entry(&provider)?.get_password()
        .map_err(|_| format!("No {provider} API key is saved yet."))?;
    let client = Client::builder().connect_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(45))
        .build().map_err(|error| format!("Could not create the provider connection: {error}"))?;
    let mut request = match provider.as_str() {
        "gemini" => client.get("https://generativelanguage.googleapis.com/v1beta/models").header("x-goog-api-key", api_key),
        "openai" => client.get("https://api.openai.com/v1/models").bearer_auth(api_key),
        "anthropic" => client.get("https://api.anthropic.com/v1/models").header("x-api-key", api_key).header("anthropic-version", "2023-06-01"),
        _ => return Err("Choose Gemini, OpenAI, or Anthropic.".to_string()),
    };
    request = request.header("accept", "application/json");
    let response = request.send().await.map_err(|_| format!("Could not reach {provider}. Check your internet connection and API key."))?;
    let status = response.status();
    let payload: Value = response.json().await.map_err(|_| format!("Could not read the model list returned by {provider}."))?;
    if !status.is_success() { return Err(provider_error(status, payload.to_string())); }
    let returned_models: Vec<String> = match provider.as_str() {
        "gemini" => payload["models"].as_array().into_iter().flatten()
            .filter(|item| item["supportedGenerationMethods"].as_array().is_some_and(|methods| methods.iter().any(|method| method.as_str() == Some("generateContent"))))
            .filter_map(|item| item["name"].as_str().map(|name| name.strip_prefix("models/").unwrap_or(name).to_string())).collect(),
        "openai" => payload["data"].as_array().into_iter().flatten()
            .filter_map(|item| item["id"].as_str()).filter(|id| id.starts_with("gpt-") || id.starts_with('o') || id.starts_with("chat-")).map(str::to_string).collect(),
        "anthropic" => payload["data"].as_array().into_iter().flatten().filter_map(|item| item["id"].as_str()).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    let preferred: &[&str] = match provider.as_str() {
        "gemini" => &["gemini-4-hydrogen", "gemini-3.8-flash", "gemini-3.6-flash", "gemini-3.5-flash-lite"],
        "openai" => &["gpt-6.2-sol", "gpt-6-luna", "gpt-6.2-astra"],
        "anthropic" => &["claude-sonnet-5-5", "claude-haiku-5-5", "claude-opus-5-5"],
        _ => &[],
    };
    let mut models: Vec<String> = preferred.iter().filter(|id| returned_models.iter().any(|available| available.as_str() == **id)).map(|id| (*id).to_string()).collect();
    // Some provider accounts return preview/tooling models that list text generation but reject
    // multi-turn chat. Offer only our known chat-compatible IDs, with documented choices as a
    // fallback when the provider's model catalog omits them for this key or region.
    if models.is_empty() { models = preferred.iter().map(|id| (*id).to_string()).collect(); }
    Ok(models)
}

fn provider_error(status: reqwest::StatusCode, detail: String) -> String {
    format!("Provider request failed with {status}: {detail}")
}

#[tauri::command]
async fn stream_provider_chat(
    app: AppHandle,
    request_id: String,
    provider: String,
    model: String,
    messages: Vec<ProviderMessage>,
) -> Result<(), String> {
    let api_key = provider_key_entry(&provider)?
        .get_password()
        .map_err(|_| format!("No {provider} API key is saved. Add one in Provider settings first."))?;
    if model.trim().is_empty() || messages.is_empty() {
        return Err("Choose a model and enter a message before sending.".to_string());
    }
    let client = Client::builder().connect_timeout(Duration::from_secs(15)).timeout(Duration::from_secs(300))
        .build().map_err(|error| format!("Could not create the provider connection: {error}"))?;
    let (url, body) = match provider.as_str() {
        "gemini" => {
            if !model.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')) {
                return Err("The Gemini model ID contains unsupported characters.".to_string());
            }
            let system = messages.iter().filter(|item| item.role == "system").map(|item| item.content.as_str()).collect::<Vec<_>>().join("\n\n");
            let contents: Vec<Value> = messages.iter().filter(|item| item.role != "system").map(|item| {
                json!({ "role": if item.role == "assistant" { "model" } else { "user" }, "parts": [{ "text": item.content }] })
            }).collect();
            let mut body = json!({ "contents": contents });
            if !system.is_empty() { body["systemInstruction"] = json!({ "parts": [{ "text": system }] }); }
            (format!("https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent"), body)
        },
        "openai" => {
            let input: Vec<Value> = messages.iter().map(|item| json!({"role": if item.role == "system" { "developer" } else { item.role.as_str() }, "content": item.content})).collect();
            ("https://api.openai.com/v1/responses".to_string(), json!({"model": model, "input": input}))
        },
        "anthropic" => {
            let system = messages.iter().filter(|item| item.role == "system").map(|item| item.content.as_str()).collect::<Vec<_>>().join("\n\n");
            let input: Vec<Value> = messages.iter().filter(|item| item.role != "system").map(|item| json!({"role": item.role, "content": item.content})).collect();
            let mut body = json!({"model": model, "max_tokens": 2048, "messages": input});
            if !system.is_empty() { body["system"] = json!(system); }
            ("https://api.anthropic.com/v1/messages".to_string(), body)
        },
        _ => return Err("This provider is not supported yet.".to_string()),
    };
    let mut request = client.post(url).json(&body);
    match provider.as_str() {
        "gemini" => request = request.header("x-goog-api-key", api_key),
        "openai" => request = request.bearer_auth(api_key),
        "anthropic" => request = request.header("x-api-key", api_key).header("anthropic-version", "2023-06-01"),
        _ => {}
    }
    let response = request.send().await.map_err(|_| format!("Could not reach {provider}. Check your internet connection and API key."))?;
    let status = response.status();
    let payload: Value = response.json().await.map_err(|error| format!("Could not decode {provider}'s response: {error}"))?;
    if !status.is_success() { return Err(provider_error(status, payload.to_string())); }
    let answer = match provider.as_str() {
        "gemini" => payload["candidates"][0]["content"]["parts"].as_array().map(|parts| parts.iter().filter_map(|part| part["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default(),
        "openai" => payload["output"].as_array().map(|items| items.iter().flat_map(|item| item["content"].as_array().into_iter().flatten()).filter_map(|part| part["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default(),
        "anthropic" => payload["content"].as_array().map(|parts| parts.iter().filter_map(|part| part["text"].as_str()).collect::<Vec<_>>().join("\n")).unwrap_or_default(),
        _ => String::new(),
    };
    if answer.trim().is_empty() { return Err(format!("{provider} returned no text. Check model availability and provider response details.")); }
    let input_tokens = match provider.as_str() {
        "gemini" => payload["usageMetadata"]["promptTokenCount"].as_u64(),
        "openai" => payload["usage"]["input_tokens"].as_u64(),
        "anthropic" => payload["usage"]["input_tokens"].as_u64(),
        _ => None,
    };
    let output_tokens = match provider.as_str() {
        "gemini" => payload["usageMetadata"]["candidatesTokenCount"].as_u64(),
        "openai" => payload["usage"]["output_tokens"].as_u64(),
        "anthropic" => payload["usage"]["output_tokens"].as_u64(),
        _ => None,
    };
    app.emit("chat-stream", ChatStreamEvent { request_id, content: answer, done: true, prompt_eval_count: input_tokens, eval_count: output_tokens, total_duration: None, load_duration: None, prompt_eval_duration: None, eval_duration: None })
        .map_err(|error| format!("Could not deliver the model response: {error}"))?;
    Ok(())
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatMetrics {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatSessionMetadata {
    id: String,
    title: String,
    model: String,
    created_at: u64,
    updated_at: u64,
    message_count: usize,
    context_summary: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LoadedChatSession {
    metadata: ChatSessionMetadata,
    messages: Vec<ChatMessage>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryIndexResult {
    indexed_count: usize,
    total_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct HistorySearchHit {
    session_id: String,
    title: String,
    content: String,
    created_at: u64,
    score: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DocumentMetadata {
    id: String,
    name: String,
    page_count: usize,
    extracted_chars: usize,
    created_at: u64,
    updated_at: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct DocumentPage {
    page_number: usize,
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredDocument {
    metadata: DocumentMetadata,
    pages: Vec<DocumentPage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DocumentChunkInput {
    page_number: usize,
    index: usize,
    text: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentIndexResult {
    indexed_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DocumentSearchHit {
    page_number: usize,
    chunk_index: usize,
    text: String,
    score: f32,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ExperimentMetricEntry {
    id: String,
    experiment_id: String,
    label: String,
    mode: String,
    model: String,
    document_name: String,
    question: String,
    created_at: u64,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_duration_ns: Option<u64>,
    load_duration_ns: Option<u64>,
    eval_duration_ns: Option<u64>,
    client_duration_ns: Option<u64>,
    retrieved_chunks: usize,
    status: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChatUsageEntry {
    message_id: String,
    session_id: String,
    title: String,
    model: String,
    updated_at: u64,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Deserialize)]
struct LegacyEmbeddingResponse {
    embedding: Vec<f32>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ChatStreamEvent {
    request_id: String,
    content: String,
    done: bool,
    prompt_eval_count: Option<u64>,
    eval_count: Option<u64>,
    total_duration: Option<u64>,
    load_duration: Option<u64>,
    prompt_eval_duration: Option<u64>,
    eval_duration: Option<u64>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OllamaPullEvent {
    request_id: String,
    status: String,
    total: Option<u64>,
    completed: Option<u64>,
    done: bool,
}

fn current_time_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn chats_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not locate ContextCraft's local data folder: {error}"))?
        .join("chats");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not prepare local chat storage: {error}"))?;
    Ok(directory)
}

fn documents_directory(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not locate ContextCraft's local data folder: {error}"))?
        .join("documents");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not prepare local document storage: {error}"))?;
    Ok(directory)
}

fn document_directory(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    if id.len() != 36 || !id.chars().all(|character| character.is_ascii_hexdigit() || character == '-') {
        return Err("This document has an invalid identifier.".to_string());
    }
    Ok(documents_directory(app)?.join(id))
}

fn memory_database_path(app: &AppHandle) -> Result<PathBuf, String> {
    let directory = app
        .path()
        .app_local_data_dir()
        .map_err(|error| format!("Could not locate ContextCraft's local data folder: {error}"))?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not prepare local memory storage: {error}"))?;
    Ok(directory.join("chat-memory.sqlite"))
}

fn open_memory_database(app: &AppHandle) -> Result<Connection, String> {
    let path = memory_database_path(app)?;
    let connection = Connection::open(path)
        .map_err(|error| format!("Could not open the local chat vector database: {error}"))?;
    connection
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS chat_vectors (
                message_key TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                title TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                embedding BLOB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chat_vectors_session ON chat_vectors(session_id);
            CREATE INDEX IF NOT EXISTS idx_chat_vectors_created ON chat_vectors(created_at);
            CREATE TABLE IF NOT EXISTS document_vectors (
                vector_key TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                page_number INTEGER NOT NULL,
                chunk_index INTEGER NOT NULL,
                content TEXT NOT NULL,
                embedding BLOB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_document_vectors_document ON document_vectors(document_id);
            CREATE TABLE IF NOT EXISTS experiment_metrics (
                id TEXT PRIMARY KEY,
                experiment_id TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                payload TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_experiment_metrics_group ON experiment_metrics(experiment_id);
            CREATE INDEX IF NOT EXISTS idx_experiment_metrics_created ON experiment_metrics(created_at);",
        )
        .map_err(|error| format!("Could not prepare the local chat vector database: {error}"))?;
    Ok(connection)
}

fn delete_session_vectors(app: &AppHandle, id: &str) -> Result<(), String> {
    let path = memory_database_path(app)?;
    if !path.exists() {
        return Ok(());
    }
    let connection = Connection::open(path)
        .map_err(|error| format!("Could not open the local chat vector database: {error}"))?;
    connection
        .execute("DELETE FROM chat_vectors WHERE session_id = ?1", [id])
        .map_err(|error| format!("Could not remove this chat from semantic memory: {error}"))?;
    Ok(())
}

fn refresh_session_vector_title(app: &AppHandle, id: &str, title: &str) -> Result<(), String> {
    let path = memory_database_path(app)?;
    if !path.exists() {
        return Ok(());
    }
    let connection = Connection::open(path)
        .map_err(|error| format!("Could not open the local chat vector database: {error}"))?;
    connection
        .execute("UPDATE chat_vectors SET title = ?1 WHERE session_id = ?2", params![title, id])
        .map_err(|error| format!("Could not update the chat memory title: {error}"))?;
    Ok(())
}

fn vector_bytes(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|value| value.to_le_bytes()).collect()
}

fn vector_from_bytes(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

fn cosine_similarity(left: &[f32], right: &[f32]) -> f32 {
    if left.len() != right.len() || left.is_empty() {
        return -1.0;
    }
    let mut dot = 0.0_f32;
    let mut left_norm = 0.0_f32;
    let mut right_norm = 0.0_f32;
    for (a, b) in left.iter().zip(right.iter()) {
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }
    let denominator = left_norm.sqrt() * right_norm.sqrt();
    if denominator == 0.0 { -1.0 } else { dot / denominator }
}

async fn create_embeddings(base_url: &str, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(180))
        .build()
        .map_err(|error| format!("Could not create the Ollama embedding connection: {error}"))?;
    let modern_response = client
        .post(format!("{base_url}/api/embed"))
        .json(&json!({ "model": "nomic-embed-text", "input": texts }))
        .send()
        .await
        .map_err(|error| format!("Could not reach Ollama's embedding endpoint: {error}"))?;

    let embeddings = if modern_response.status() == reqwest::StatusCode::NOT_FOUND {
        // Older Ollama releases expose the singular /api/embeddings endpoint.
        // Fall back to one prompt per request so those versions remain usable.
        let mut legacy_vectors = Vec::with_capacity(texts.len());
        for text in texts {
            let response = client
                .post(format!("{base_url}/api/embeddings"))
                .json(&json!({ "model": "nomic-embed-text", "prompt": text }))
                .send()
                .await
                .map_err(|error| format!("Could not reach Ollama's legacy embedding endpoint: {error}"))?;
            let status = response.status();
            if !status.is_success() {
                let detail = response.text().await.unwrap_or_default();
                return Err(format!("Ollama's legacy embedding endpoint returned {status}. {detail}"));
            }
            let result = response
                .json::<LegacyEmbeddingResponse>()
                .await
                .map_err(|error| format!("Could not read an embedding from Ollama's legacy endpoint: {error}"))?;
            legacy_vectors.push(result.embedding);
        }
        legacy_vectors
    } else {
        let response = modern_response
            .error_for_status()
            .map_err(|error| format!("Ollama could not create embeddings. Is nomic-embed-text installed? {error}"))?
            .json::<EmbeddingResponse>()
            .await
            .map_err(|error| format!("Could not read embeddings from Ollama: {error}"))?;
        if response.embeddings.len() != texts.len() {
            return Err("Ollama returned a different number of embeddings than requested.".to_string());
        }
        response.embeddings
    };

    if embeddings.iter().any(|embedding| embedding.len() != 768) {
        return Err("The selected embedding model returned an unexpected vector size; this index expects nomic-embed-text's 768 dimensions.".to_string());
    }
    Ok(embeddings)
}

fn chat_exchange_chunks(loaded: &LoadedChatSession) -> Vec<(String, String)> {
    let mut chunks = Vec::new();
    for (index, message) in loaded.messages.iter().enumerate() {
        if message.role != "user" {
            continue;
        }
        let Some(answer) = loaded.messages.iter().skip(index + 1).find(|item| item.role == "assistant") else {
            continue;
        };
        if answer.content.trim().is_empty() {
            continue;
        }
        let text = format!("Question: {}\nAnswer: {}", message.content, answer.content);
        let shortened: String = text.chars().take(6000).collect();
        chunks.push((answer.id.clone(), shortened));
    }
    chunks
}

async fn index_one_chat(app: &AppHandle, base_url: &str, id: &str) -> Result<usize, String> {
    let directory = chat_directory(app, id)?;
    let metadata = read_chat_metadata(&directory)?;
    let loaded = load_chat_session(app.clone(), id.to_string())?;
    let chunks = chat_exchange_chunks(&loaded);
    if chunks.is_empty() {
        return Ok(0);
    }

    let connection = open_memory_database(app)?;
    let mut missing = Vec::new();
    for (key, text) in chunks {
        let already_indexed: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM chat_vectors WHERE message_key = ?1)",
                [&key],
                |row| row.get(0),
            )
            .map_err(|error| format!("Could not inspect the local vector index: {error}"))?;
        if already_indexed {
            connection
                .execute("UPDATE chat_vectors SET title = ?1 WHERE message_key = ?2", params![metadata.title, key])
                .map_err(|error| format!("Could not update the chat index title: {error}"))?;
        } else {
            missing.push((key, text));
        }
    }
    drop(connection);
    if missing.is_empty() {
        return Ok(0);
    }

    let texts: Vec<String> = missing.iter().map(|(_, text)| text.clone()).collect();
    let embeddings = create_embeddings(base_url, &texts).await?;
    let mut connection = open_memory_database(app)?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("Could not start a vector index update: {error}"))?;
    for ((key, text), embedding) in missing.iter().zip(embeddings.iter()) {
        transaction
            .execute(
                "INSERT OR REPLACE INTO chat_vectors(message_key, session_id, title, content, created_at, embedding) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![key, id, metadata.title, text, metadata.updated_at, vector_bytes(embedding)],
            )
            .map_err(|error| format!("Could not save a chat vector: {error}"))?;
    }
    transaction
        .commit()
        .map_err(|error| format!("Could not finish updating the vector index: {error}"))?;
    Ok(missing.len())
}

fn chat_directory(app: &AppHandle, id: &str) -> Result<PathBuf, String> {
    if id.len() != 36 || !id.chars().all(|character| character.is_ascii_hexdigit() || character == '-') {
        return Err("This chat has an invalid identifier.".to_string());
    }
    Ok(chats_directory(app)?.join(id))
}

fn read_chat_metadata(directory: &PathBuf) -> Result<ChatSessionMetadata, String> {
    let path = directory.join("context.json");
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read chat context at {}: {error}", path.display()))?;
    serde_json::from_str(&contents)
        .map_err(|error| format!("Could not decode chat context at {}: {error}", path.display()))
}

fn write_json_file<T: Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    let contents = serde_json::to_vec_pretty(value)
        .map_err(|error| format!("Could not prepare local chat data: {error}"))?;
    fs::write(path, contents)
        .map_err(|error| format!("Could not write local chat data at {}: {error}", path.display()))
}

#[tauri::command]
fn list_documents(app: AppHandle) -> Result<Vec<DocumentMetadata>, String> {
    let root = documents_directory(&app)?;
    let entries = fs::read_dir(&root)
        .map_err(|error| format!("Could not list saved documents: {error}"))?;
    let mut documents = Vec::new();
    for entry in entries.flatten() {
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            let path = entry.path().join("document.json");
            if let Ok(contents) = fs::read_to_string(path) {
                if let Ok(document) = serde_json::from_str::<StoredDocument>(&contents) {
                    documents.push(document.metadata);
                }
            }
        }
    }
    documents.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(documents)
}

#[tauri::command]
fn load_document(app: AppHandle, id: String) -> Result<StoredDocument, String> {
    let path = document_directory(&app, &id)?.join("document.json");
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read saved document text: {error}"))?;
    serde_json::from_str(&contents)
        .map_err(|error| format!("Could not decode saved document text: {error}"))
}

#[tauri::command]
fn save_document(
    app: AppHandle,
    id: String,
    name: String,
    pages: Vec<DocumentPage>,
) -> Result<DocumentMetadata, String> {
    if pages.is_empty() || pages.len() > 600 {
        return Err("The document must contain between 1 and 600 pages or text sections.".to_string());
    }
    let total_chars: usize = pages.iter().map(|page| page.text.chars().count()).sum();
    if total_chars > 20_000_000 {
        return Err("This document has too much extracted text for this checkpoint (limit: 20 million characters).".to_string());
    }
    let directory = document_directory(&app, &id)?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create local document storage: {error}"))?;
    let now = current_time_millis();
    let previous = fs::read_to_string(directory.join("document.json"))
        .ok()
        .and_then(|contents| serde_json::from_str::<StoredDocument>(&contents).ok());
    let metadata = DocumentMetadata {
        id,
        name: if name.trim().is_empty() { "Untitled file".to_string() } else { name.trim().to_string() },
        page_count: pages.len(),
        extracted_chars: total_chars,
        created_at: previous.map(|document| document.metadata.created_at).unwrap_or(now),
        updated_at: now,
    };
    write_json_file(
        &directory.join("document.json"),
        &StoredDocument { metadata: metadata.clone(), pages },
    )?;
    Ok(metadata)
}

#[tauri::command]
fn delete_document(app: AppHandle, id: String) -> Result<(), String> {
    let directory = document_directory(&app, &id)?;
    let connection = open_memory_database(&app)?;
    connection
        .execute("DELETE FROM document_vectors WHERE document_id = ?1", [&id])
        .map_err(|error| format!("Could not remove this PDF's vector index: {error}"))?;
    if directory.exists() {
        fs::remove_dir_all(directory)
            .map_err(|error| format!("Could not delete this saved document: {error}"))?;
    }
    Ok(())
}

#[tauri::command]
fn list_chat_sessions(app: AppHandle) -> Result<Vec<ChatSessionMetadata>, String> {
    let root = chats_directory(&app)?;
    let entries = fs::read_dir(&root)
        .map_err(|error| format!("Could not list saved chats: {error}"))?;
    let mut sessions = Vec::new();
    for entry in entries.flatten() {
        if entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            if let Ok(metadata) = read_chat_metadata(&entry.path()) {
                sessions.push(metadata);
            }
        }
    }
    sessions.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(sessions)
}

#[tauri::command]
fn create_chat_session(app: AppHandle, id: String, model: String) -> Result<ChatSessionMetadata, String> {
    let directory = chat_directory(&app, &id)?;
    fs::create_dir_all(&directory)
        .map_err(|error| format!("Could not create a new chat: {error}"))?;
    let now = current_time_millis();
    let metadata = ChatSessionMetadata {
        id,
        title: "New chat".to_string(),
        model,
        created_at: now,
        updated_at: now,
        message_count: 0,
        context_summary: String::new(),
    };
    write_json_file(&directory.join("context.json"), &metadata)?;
    write_json_file(&directory.join("messages.json"), &Vec::<ChatMessage>::new())?;
    Ok(metadata)
}

#[tauri::command]
fn load_chat_session(app: AppHandle, id: String) -> Result<LoadedChatSession, String> {
    let directory = chat_directory(&app, &id)?;
    let metadata = read_chat_metadata(&directory)?;
    let path = directory.join("messages.json");
    let contents = fs::read_to_string(&path)
        .map_err(|error| format!("Could not read saved messages: {error}"))?;
    let messages = serde_json::from_str(&contents)
        .map_err(|error| format!("Could not decode saved messages: {error}"))?;
    Ok(LoadedChatSession { metadata, messages })
}

#[tauri::command]
fn save_chat_session(
    app: AppHandle,
    id: String,
    title: String,
    model: String,
    context_summary: String,
    messages: Vec<ChatMessage>,
) -> Result<ChatSessionMetadata, String> {
    let directory = chat_directory(&app, &id)?;
    let mut metadata = read_chat_metadata(&directory)?;
    metadata.title = if title.trim().is_empty() { "New chat".to_string() } else { title.trim().to_string() };
    metadata.model = model;
    metadata.context_summary = context_summary;
    metadata.message_count = messages.len();
    metadata.updated_at = current_time_millis();
    write_json_file(&directory.join("messages.json"), &messages)?;
    write_json_file(&directory.join("context.json"), &metadata)?;
    if messages.is_empty() {
        delete_session_vectors(&app, &id)?;
    } else {
        refresh_session_vector_title(&app, &id, &metadata.title)?;
    }
    Ok(metadata)
}

#[tauri::command]
fn delete_chat_session(app: AppHandle, id: String) -> Result<(), String> {
    let directory = chat_directory(&app, &id)?;
    delete_session_vectors(&app, &id)?;
    if directory.exists() {
        fs::remove_dir_all(directory).map_err(|error| format!("Could not delete this chat: {error}"))?;
    }
    Ok(())
}

#[tauri::command]
async fn index_chat_history(app: AppHandle, base_url: String) -> Result<HistoryIndexResult, String> {
    let sessions = list_chat_sessions(app.clone())?;
    let mut indexed_count = 0;
    for session in &sessions {
        indexed_count += index_one_chat(&app, base_url.trim().trim_end_matches('/'), &session.id).await?;
    }
    let connection = open_memory_database(&app)?;
    let total_count = connection
        .query_row("SELECT COUNT(*) FROM chat_vectors", [], |row| row.get(0))
        .map_err(|error| format!("Could not count indexed chat memories: {error}"))?;
    Ok(HistoryIndexResult { indexed_count, total_count })
}

#[tauri::command]
fn get_indexed_memory_count(app: AppHandle) -> Result<usize, String> {
    let connection = open_memory_database(&app)?;
    connection
        .query_row("SELECT COUNT(*) FROM chat_vectors", [], |row| row.get(0))
        .map_err(|error| format!("Could not count indexed chat memories: {error}"))
}

#[tauri::command]
async fn index_document_chunks(
    app: AppHandle,
    base_url: String,
    document_id: String,
    chunks: Vec<DocumentChunkInput>,
) -> Result<DocumentIndexResult, String> {
    let _ = load_document(app.clone(), document_id.clone())?;
    let chunks: Vec<DocumentChunkInput> = chunks
        .into_iter()
        .filter(|chunk| !chunk.text.trim().is_empty())
        .collect();
    if chunks.is_empty() {
        return Err("There is no extracted text to index in this PDF.".to_string());
    }
    if chunks.len() > 2500 {
        return Err("This PDF has too many chunks for this checkpoint (limit: 2,500).".to_string());
    }

    let normalized = base_url.trim().trim_end_matches('/');
    let mut vectors: Vec<Vec<f32>> = Vec::with_capacity(chunks.len());
    for batch in chunks.chunks(16) {
        let texts: Vec<String> = batch.iter().map(|chunk| chunk.text.clone()).collect();
        vectors.extend(create_embeddings(normalized, &texts).await?);
    }

    let mut connection = open_memory_database(&app)?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("Could not start updating the PDF vector index: {error}"))?;
    transaction
        .execute("DELETE FROM document_vectors WHERE document_id = ?1", [&document_id])
        .map_err(|error| format!("Could not replace the previous PDF index: {error}"))?;
    for (chunk, embedding) in chunks.iter().zip(vectors.iter()) {
        let key = format!("{}:{}:{}", document_id, chunk.page_number, chunk.index);
        transaction
            .execute(
                "INSERT INTO document_vectors(vector_key, document_id, page_number, chunk_index, content, embedding) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![key, document_id, chunk.page_number as i64, chunk.index as i64, chunk.text, vector_bytes(embedding)],
            )
            .map_err(|error| format!("Could not save an embedded PDF chunk: {error}"))?;
    }
    transaction
        .commit()
        .map_err(|error| format!("Could not finish the PDF vector index: {error}"))?;
    Ok(DocumentIndexResult { indexed_count: chunks.len() })
}

#[tauri::command]
fn get_document_index_count(app: AppHandle, document_id: String) -> Result<usize, String> {
    let _ = document_directory(&app, &document_id)?;
    let connection = open_memory_database(&app)?;
    connection
        .query_row(
            "SELECT COUNT(*) FROM document_vectors WHERE document_id = ?1",
            [&document_id],
            |row| row.get(0),
        )
        .map_err(|error| format!("Could not count indexed PDF chunks: {error}"))
}

#[tauri::command]
async fn search_document_chunks(
    app: AppHandle,
    base_url: String,
    document_id: String,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<DocumentSearchHit>, String> {
    let _ = document_directory(&app, &document_id)?;
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let query_embeddings = create_embeddings(
        base_url.trim().trim_end_matches('/'),
        &[query.trim().to_string()],
    )
    .await?;
    let query_vector = &query_embeddings[0];
    let connection = open_memory_database(&app)?;
    let mut statement = connection
        .prepare("SELECT page_number, chunk_index, content, embedding FROM document_vectors WHERE document_id = ?1")
        .map_err(|error| format!("Could not search the PDF chunk index: {error}"))?;
    let rows = statement
        .query_map([&document_id], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
            ))
        })
        .map_err(|error| format!("Could not read the PDF chunk index: {error}"))?;
    let mut hits = Vec::new();
    for row in rows {
        let (page_number, chunk_index, text, bytes) =
            row.map_err(|error| format!("Could not read an indexed PDF chunk: {error}"))?;
        let Some(vector) = vector_from_bytes(&bytes) else { continue };
        let score = cosine_similarity(query_vector, &vector);
        hits.push(DocumentSearchHit { page_number: page_number as usize, chunk_index: chunk_index as usize, text, score });
    }
    hits.sort_by(|left, right| right.score.total_cmp(&left.score));
    hits.truncate(limit.unwrap_or(5).clamp(1, 10));
    Ok(hits)
}

#[tauri::command]
fn save_experiment_metrics(
    app: AppHandle,
    entries: Vec<ExperimentMetricEntry>,
) -> Result<usize, String> {
    if entries.is_empty() || entries.len() > 3 {
        return Err("An experiment must contain between 1 and 3 result records.".to_string());
    }
    let experiment_id = &entries[0].experiment_id;
    if entries.iter().any(|entry| &entry.experiment_id != experiment_id) {
        return Err("All saved comparison results must belong to the same experiment.".to_string());
    }
    let mut connection = open_memory_database(&app)?;
    let transaction = connection
        .transaction()
        .map_err(|error| format!("Could not start saving comparison metrics: {error}"))?;
    for entry in &entries {
        let payload = serde_json::to_string(entry)
            .map_err(|error| format!("Could not encode comparison metrics: {error}"))?;
        transaction
            .execute(
                "INSERT OR REPLACE INTO experiment_metrics(id, experiment_id, created_at, payload) VALUES (?1, ?2, ?3, ?4)",
                params![entry.id, entry.experiment_id, entry.created_at as i64, payload],
            )
            .map_err(|error| format!("Could not save comparison metrics: {error}"))?;
    }
    transaction
        .commit()
        .map_err(|error| format!("Could not finish saving comparison metrics: {error}"))?;
    Ok(entries.len())
}

#[tauri::command]
fn list_experiment_metrics(
    app: AppHandle,
    limit: Option<usize>,
) -> Result<Vec<ExperimentMetricEntry>, String> {
    let connection = open_memory_database(&app)?;
    let mut statement = connection
        .prepare("SELECT payload FROM experiment_metrics ORDER BY created_at DESC LIMIT ?1")
        .map_err(|error| format!("Could not prepare comparison history: {error}"))?;
    let rows = statement
        .query_map([limit.unwrap_or(300).clamp(1, 1000) as i64], |row| row.get::<_, String>(0))
        .map_err(|error| format!("Could not read comparison history: {error}"))?;
    let mut entries = Vec::new();
    for row in rows {
        let payload = row.map_err(|error| format!("Could not read a comparison record: {error}"))?;
        entries.push(
            serde_json::from_str::<ExperimentMetricEntry>(&payload)
                .map_err(|error| format!("Could not decode a comparison record: {error}"))?,
        );
    }
    Ok(entries)
}

#[tauri::command]
fn delete_experiment_group(app: AppHandle, experiment_id: String) -> Result<(), String> {
    let connection = open_memory_database(&app)?;
    connection
        .execute("DELETE FROM experiment_metrics WHERE experiment_id = ?1", [&experiment_id])
        .map_err(|error| format!("Could not delete this comparison: {error}"))?;
    Ok(())
}

#[tauri::command]
fn clear_experiment_metrics(app: AppHandle) -> Result<(), String> {
    let connection = open_memory_database(&app)?;
    connection
        .execute("DELETE FROM experiment_metrics", [])
        .map_err(|error| format!("Could not clear comparison history: {error}"))?;
    Ok(())
}

#[tauri::command]
fn list_chat_usage_metrics(app: AppHandle) -> Result<Vec<ChatUsageEntry>, String> {
    let sessions = list_chat_sessions(app.clone())?;
    let mut entries = Vec::new();
    for session in sessions {
        let loaded = load_chat_session(app.clone(), session.id.clone())?;
        for message in loaded.messages {
            if message.role != "assistant" || message.content.trim().is_empty() {
                continue;
            }
            entries.push(ChatUsageEntry {
                message_id: message.id,
                session_id: session.id.clone(),
                title: session.title.clone(),
                model: message.model.unwrap_or_else(|| session.model.clone()),
                updated_at: session.updated_at,
                input_tokens: message.metrics.as_ref().and_then(|metrics| metrics.input_tokens),
                output_tokens: message.metrics.as_ref().and_then(|metrics| metrics.output_tokens),
            });
        }
    }
    entries.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    entries.truncate(500);
    Ok(entries)
}

#[tauri::command]
async fn search_chat_memories(
    app: AppHandle,
    base_url: String,
    query: String,
    exclude_session_id: Option<String>,
    limit: Option<usize>,
) -> Result<Vec<HistorySearchHit>, String> {
    let normalized = base_url.trim().trim_end_matches('/');
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let query_embeddings = create_embeddings(normalized, &[query.trim().to_string()]).await?;
    let query_vector = &query_embeddings[0];
    let connection = open_memory_database(&app)?;
    let mut statement = connection
        .prepare("SELECT session_id, title, content, created_at, embedding FROM chat_vectors")
        .map_err(|error| format!("Could not search the local vector index: {error}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, u64>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        })
        .map_err(|error| format!("Could not read local chat memories: {error}"))?;
    let mut hits = Vec::new();
    for row in rows {
        let (session_id, title, content, created_at, bytes) =
            row.map_err(|error| format!("Could not read a chat memory row: {error}"))?;
        if exclude_session_id.as_deref() == Some(session_id.as_str()) {
            continue;
        }
        let Some(vector) = vector_from_bytes(&bytes) else { continue };
        let score = cosine_similarity(query_vector, &vector);
        hits.push(HistorySearchHit { session_id, title, content, created_at, score });
    }
    hits.sort_by(|left, right| right.score.total_cmp(&left.score));
    hits.truncate(limit.unwrap_or(5).clamp(1, 10));
    Ok(hits)
}

#[tauri::command]
async fn get_ollama_status(base_url: String) -> Result<OllamaStatus, String> {
    let normalized = base_url.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(normalized).map_err(|_| "Enter a valid Ollama URL, such as http://localhost:11434".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Ollama URL must use http:// or https://".to_string());
    }

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(7))
        .build()
        .map_err(|error| format!("Could not create the Ollama connection: {error}"))?;

    let version_response = client
        .get(format!("{normalized}/api/version"))
        .send()
        .await
        .map_err(|error| format!("Could not connect to Ollama at {normalized}. Make sure Ollama is running. ({error})"))?
        .error_for_status()
        .map_err(|error| format!("Ollama returned an error while checking its version: {error}"))?
        .json::<VersionResponse>()
        .await
        .map_err(|error| format!("Could not read the Ollama version response: {error}"))?;

    let tags = client
        .get(format!("{normalized}/api/tags"))
        .send()
        .await
        .map_err(|error| format!("Could not list Ollama models: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Ollama returned an error while listing models: {error}"))?
        .json::<TagsResponse>()
        .await
        .map_err(|error| format!("Could not read the model list from Ollama: {error}"))?;

    let running = client
        .get(format!("{normalized}/api/ps"))
        .send()
        .await
        .map_err(|error| format!("Could not list running Ollama models: {error}"))?
        .error_for_status()
        .map_err(|error| format!("Ollama returned an error while listing running models: {error}"))?
        .json::<RunningResponse>()
        .await
        .map_err(|error| format!("Could not read the running model list from Ollama: {error}"))?;

    let running_models = running
        .models
        .into_iter()
        .filter_map(|model| model.name.or(model.model))
        .collect();

    Ok(OllamaStatus {
        base_url: normalized.to_string(),
        version: version_response.version,
        models: tags.models,
        running_models,
    })
}

fn emit_chat_line(app: &AppHandle, request_id: &str, line: &[u8]) -> Result<bool, String> {
    let payload: Value = serde_json::from_slice(line)
        .map_err(|error| format!("Could not decode Ollama's chat stream: {error}"))?;
    let content = payload
        .get("message")
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let done = payload.get("done").and_then(Value::as_bool).unwrap_or(false);
    let prompt_eval_count = payload.get("prompt_eval_count").and_then(Value::as_u64);
    let eval_count = payload.get("eval_count").and_then(Value::as_u64);
    let total_duration = payload.get("total_duration").and_then(Value::as_u64);
    let load_duration = payload.get("load_duration").and_then(Value::as_u64);
    let prompt_eval_duration = payload.get("prompt_eval_duration").and_then(Value::as_u64);
    let eval_duration = payload.get("eval_duration").and_then(Value::as_u64);

    app.emit(
        "chat-stream",
        ChatStreamEvent {
            request_id: request_id.to_string(),
            content,
            done,
            prompt_eval_count,
            eval_count,
            total_duration,
            load_duration,
            prompt_eval_duration,
            eval_duration,
        },
    )
    .map_err(|error| format!("Could not deliver the model response to ContextCraft: {error}"))?;

    Ok(done)
}

#[tauri::command]
async fn stream_chat(
    app: AppHandle,
    request_id: String,
    base_url: String,
    model: String,
    messages: Vec<OllamaMessage>,
) -> Result<(), String> {
    let normalized = base_url.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(normalized)
        .map_err(|_| "Enter a valid Ollama URL, such as http://localhost:11434".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err("Ollama URL must use http:// or https://".to_string());
    }
    if model.trim().is_empty() {
        return Err("Choose an installed Ollama model before sending a message.".to_string());
    }
    if messages.is_empty() {
        return Err("Enter a message before sending.".to_string());
    }

    let client = Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|error| format!("Could not create the Ollama connection: {error}"))?;

    let mut response = client
        .post(format!("{normalized}/api/chat"))
        .json(&json!({
            "model": model,
            "messages": messages,
            "stream": true,
            "think": false,
            "options": { "num_ctx": 4096 }
        }))
        .send()
        .await
        .map_err(|error| format!("Could not send the message to Ollama: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(format!("Ollama chat request failed with {status}: {detail}"));
    }

    let mut buffer: Vec<u8> = Vec::new();
    let mut completed = false;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("The Ollama response stream was interrupted: {error}"))?
    {
        buffer.extend_from_slice(&chunk);
        while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = buffer.drain(..=newline).collect();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if !line.is_empty() {
                completed = emit_chat_line(&app, &request_id, &line)?;
            }
        }
    }

    if !buffer.is_empty() {
        completed = emit_chat_line(&app, &request_id, &buffer)?;
    }
    if !completed {
        return Err("Ollama closed the response stream before it finished.".to_string());
    }
    Ok(())
}

fn emit_pull_line(app: &AppHandle, request_id: &str, line: &[u8]) -> Result<bool, String> {
    let payload: Value = serde_json::from_slice(line).map_err(|error| format!("Could not read Ollama's model download progress: {error}"))?;
    if let Some(error) = payload.get("error").and_then(Value::as_str) { return Err(format!("Ollama could not pull this model: {error}")); }
    let status = payload.get("status").and_then(Value::as_str).unwrap_or("Downloading model").to_string();
    let done = status.eq_ignore_ascii_case("success");
    app.emit("ollama-pull-progress", OllamaPullEvent {
        request_id: request_id.to_string(),
        status,
        total: payload.get("total").and_then(Value::as_u64),
        completed: payload.get("completed").and_then(Value::as_u64),
        done,
    }).map_err(|error| format!("Could not update model download progress: {error}"))?;
    Ok(done)
}

#[tauri::command]
async fn pull_ollama_model(app: AppHandle, request_id: String, base_url: String, model: String) -> Result<(), String> {
    let normalized = base_url.trim().trim_end_matches('/');
    let parsed = reqwest::Url::parse(normalized).map_err(|_| "Enter a valid Ollama URL, such as http://localhost:11434".to_string())?;
    if !matches!(parsed.scheme(), "http" | "https") { return Err("Ollama URL must use http:// or https://".to_string()); }
    let model = model.trim();
    if model.is_empty() || model.len() > 160 || !model.chars().all(|character| character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-' | ':' | '/')) || model.contains("..") || model.contains("//") {
        return Err("Enter a valid Ollama model name, such as qwen3:0.6b.".to_string());
    }
    let client = Client::builder().connect_timeout(Duration::from_secs(10)).timeout(Duration::from_secs(14_400)).build()
        .map_err(|error| format!("Could not create the Ollama connection: {error}"))?;
    let mut response = client.post(format!("{normalized}/api/pull")).json(&json!({"model": model, "stream": true})).send().await
        .map_err(|error| format!("Could not start the Ollama model download: {error}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let detail = response.text().await.unwrap_or_default();
        return Err(format!("Ollama model pull failed with {status}: {detail}"));
    }
    let mut buffer = Vec::new();
    let mut completed = false;
    while let Some(chunk) = response.chunk().await.map_err(|error| format!("Ollama model download was interrupted: {error}"))? {
        buffer.extend_from_slice(&chunk);
        while let Some(newline) = buffer.iter().position(|byte| *byte == b'\n') {
            let mut line: Vec<u8> = buffer.drain(..=newline).collect();
            line.pop();
            if line.last() == Some(&b'\r') { line.pop(); }
            if !line.is_empty() { completed = emit_pull_line(&app, &request_id, &line)?; }
        }
    }
    if !buffer.is_empty() { completed = emit_pull_line(&app, &request_id, &buffer)?; }
    if !completed { return Err("Ollama closed the download stream before reporting success.".to_string()); }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_ollama_status,
            stream_chat,
            pull_ollama_model,
            stream_provider_chat,
            save_provider_key,
            has_provider_key,
            delete_provider_key,
            list_provider_models,
            list_chat_sessions,
            create_chat_session,
            load_chat_session,
            save_chat_session,
            delete_chat_session,
            index_chat_history,
            search_chat_memories,
            get_indexed_memory_count,
            list_documents,
            load_document,
            save_document,
            delete_document,
            index_document_chunks,
            get_document_index_count,
            search_document_chunks,
            save_experiment_metrics,
            list_experiment_metrics,
            delete_experiment_group,
            clear_experiment_metrics,
            list_chat_usage_metrics
        ])
        .run(tauri::generate_context!())
        .expect("error while running ContextCraft");
}
