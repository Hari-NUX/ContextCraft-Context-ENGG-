export interface OllamaModel {
  name: string;
  model: string;
  size: number;
  modified_at: string;
  details?: {
    family?: string;
    parameter_size?: string;
    quantization_level?: string;
  };
}

export interface OllamaStatus {
  baseUrl: string;
  version: string | null;
  models: OllamaModel[];
  runningModels: string[];
}

export type ConnectionState = "checking" | "connected" | "disconnected";
