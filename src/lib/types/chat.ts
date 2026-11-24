export interface ChatMessageInput {
  role: "system" | "user" | "assistant";
  content: string;
}

export interface ChatMetrics {
  inputTokens?: number;
  outputTokens?: number;
}

export interface ChatMessage extends ChatMessageInput {
  id: string;
  model?: string;
  metrics?: ChatMetrics;
}

export interface ChatSession {
  id: string;
  title: string;
  model: string;
  createdAt: number;
  updatedAt: number;
  messageCount: number;
  contextSummary: string;
}

export interface LoadedChatSession {
  metadata: ChatSession;
  messages: ChatMessage[];
}

export interface HistorySearchHit {
  sessionId: string;
  title: string;
  content: string;
  createdAt: number;
  score: number;
}

export interface HistoryIndexResult {
  indexedCount: number;
  totalCount: number;
}

export interface DocumentMetadata {
  id: string;
  name: string;
  folderPath?: string | null;
  pageCount: number;
  extractedChars: number;
  createdAt: number;
  updatedAt: number;
}

export interface DocumentPage {
  pageNumber: number;
  text: string;
}

export interface StoredDocument {
  metadata: DocumentMetadata;
  pages: DocumentPage[];
}

export interface DocumentChunkInput {
  pageNumber: number;
  index: number;
  text: string;
}

export interface DocumentIndexResult {
  indexedCount: number;
}

export interface DocumentSearchHit {
  pageNumber: number;
  chunkIndex: number;
  text: string;
  score: number;
}
