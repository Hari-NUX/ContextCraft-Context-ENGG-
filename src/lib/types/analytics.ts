export interface ExperimentMetricEntry {
  id: string;
  experimentId: string;
  label: string;
  mode: string;
  model: string;
  documentName: string;
  question: string;
  createdAt: number;
  inputTokens?: number;
  outputTokens?: number;
  totalDurationNs?: number;
  loadDurationNs?: number;
  evalDurationNs?: number;
  clientDurationNs?: number;
  retrievedChunks: number;
  status: string;
}

export interface ChatUsageEntry {
  messageId: string;
  sessionId: string;
  title: string;
  model: string;
  updatedAt: number;
  inputTokens?: number;
  outputTokens?: number;
}
