// Message Types
export interface Message {
  id: string;
  role: "user" | "agent" | "system";
  content: string;
  timestamp: Date;
  metadata?: {
    toolCalls?: string[];
    memories?: string[];
  };
}

// Provider Types
export type ProviderType = "anthropic" | "openai" | "ollama";

export interface ProviderInfo {
  id: ProviderType;
  name: string;
  needs_api_key: boolean;
  has_api_key: boolean;
  suggested_models: string[];
  default_model: string | null;
}

// AI Instance Types
export interface AIInstance {
  id: string;
  name: string;
  provider: ProviderType;
  model: string;
  api_base_url?: string;
  created_at: string;
  last_active: string;
}

export interface CreateInstanceRequest {
  name: string;
  provider: ProviderType;
  model: string;
  api_base_url?: string;
}

// Canvas/Program Types
export interface Program {
  id: string;
  instance_id: string;
  name: string;
  description: string;
  version: string;
  created_at: string;
  updated_at: string;
}

export type CanvasViewMode = "chat" | "split" | "canvas";
