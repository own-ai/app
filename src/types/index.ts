// Message Types
export interface Message {
  id: string;
  role: 'user' | 'agent' | 'system';
  content: string;
  timestamp: Date;
  metadata?: {
    toolCalls?: string[];
    memories?: string[];
  };
}

// AI Instance Types
export interface AIInstance {
  id: string;
  name: string;
  created_at: string;
  last_active: string;
}

export interface CreateInstanceRequest {
  name: string;
}
