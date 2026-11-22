import { invoke } from "@tauri-apps/api/core";
import type { ChatMessage, ChatMessageInput, ChatSession, DocumentChunkInput, DocumentIndexResult, DocumentMetadata, DocumentPage, DocumentSearchHit, HistoryIndexResult, HistorySearchHit, LoadedChatSession, StoredDocument } from "../types/chat";
import type { OllamaStatus } from "../types/ollama";
import type { ChatUsageEntry, ExperimentMetricEntry } from "../types/analytics";

export function getOllamaStatus(baseUrl: string): Promise<OllamaStatus> {
  return invoke<OllamaStatus>("get_ollama_status", { baseUrl });
}

export function pullOllamaModel(requestId: string, baseUrl: string, model: string): Promise<void> {
  return invoke("pull_ollama_model", { requestId, baseUrl, model });
}

export function streamOllamaChat(
  requestId: string,
  baseUrl: string,
  model: string,
  messages: ChatMessageInput[],
): Promise<void> {
  return invoke("stream_chat", { requestId, baseUrl, model, messages });
}

export function streamProviderChat(requestId: string, provider: string, model: string, messages: ChatMessageInput[]): Promise<void> {
  return invoke("stream_provider_chat", { requestId, provider, model, messages });
}

export function saveProviderKey(provider: string, apiKey: string): Promise<void> {
  return invoke("save_provider_key", { provider, apiKey });
}

export function hasProviderKey(provider: string): Promise<boolean> {
  return invoke<boolean>("has_provider_key", { provider });
}

export function deleteProviderKey(provider: string): Promise<void> {
  return invoke("delete_provider_key", { provider });
}

export function listProviderModels(provider: string): Promise<string[]> {
  return invoke<string[]>("list_provider_models", { provider });
}

export function listChatSessions(): Promise<ChatSession[]> {
  return invoke("list_chat_sessions");
}

export function createChatSession(id: string, model: string): Promise<ChatSession> {
  return invoke("create_chat_session", { id, model });
}

export function loadChatSession(id: string): Promise<LoadedChatSession> {
  return invoke("load_chat_session", { id });
}

export function saveChatSession(
  id: string,
  title: string,
  model: string,
  contextSummary: string,
  messages: ChatMessage[],
): Promise<ChatSession> {
  return invoke("save_chat_session", { id, title, model, contextSummary, messages });
}

export function deleteChatSession(id: string): Promise<void> {
  return invoke("delete_chat_session", { id });
}

export function indexChatHistory(baseUrl: string): Promise<HistoryIndexResult> {
  return invoke("index_chat_history", { baseUrl });
}

export function getIndexedMemoryCount(): Promise<number> {
  return invoke("get_indexed_memory_count");
}

export function searchChatMemories(
  baseUrl: string,
  query: string,
  excludeSessionId?: string,
  limit = 5,
): Promise<HistorySearchHit[]> {
  return invoke("search_chat_memories", { baseUrl, query, excludeSessionId, limit });
}

export function listDocuments(): Promise<DocumentMetadata[]> {
  return invoke("list_documents");
}

export function loadDocument(id: string): Promise<StoredDocument> {
  return invoke("load_document", { id });
}

export function saveDocument(id: string, name: string, pages: DocumentPage[]): Promise<DocumentMetadata> {
  return invoke("save_document", { id, name, pages });
}

export function deleteDocument(id: string): Promise<void> {
  return invoke("delete_document", { id });
}

export function indexDocumentChunks(
  baseUrl: string,
  documentId: string,
  chunks: DocumentChunkInput[],
): Promise<DocumentIndexResult> {
  return invoke("index_document_chunks", { baseUrl, documentId, chunks });
}

export function getDocumentIndexCount(documentId: string): Promise<number> {
  return invoke("get_document_index_count", { documentId });
}

export function searchDocumentChunks(
  baseUrl: string,
  documentId: string,
  query: string,
  limit = 5,
): Promise<DocumentSearchHit[]> {
  return invoke("search_document_chunks", { baseUrl, documentId, query, limit });
}

export function saveExperimentMetrics(entries: ExperimentMetricEntry[]): Promise<number> {
  return invoke("save_experiment_metrics", { entries });
}

export function listExperimentMetrics(limit = 300): Promise<ExperimentMetricEntry[]> {
  return invoke("list_experiment_metrics", { limit });
}

export function deleteExperimentGroup(experimentId: string): Promise<void> {
  return invoke("delete_experiment_group", { experimentId });
}

export function clearExperimentMetrics(): Promise<void> {
  return invoke("clear_experiment_metrics");
}

export function listChatUsageMetrics(): Promise<ChatUsageEntry[]> {
  return invoke("list_chat_usage_metrics");
}
