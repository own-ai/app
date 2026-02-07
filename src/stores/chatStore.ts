import { create } from 'zustand';
import { Message } from '@/types';

interface ChatStore {
  messages: Message[];
  isStreaming: boolean;
  
  // Actions
  addMessage: (message: Omit<Message, 'id' | 'timestamp'>) => void;
  startAgentMessage: () => void;
  appendToLastMessage: (chunk: string) => void;
  setStreaming: (streaming: boolean) => void;
  clearMessages: () => void;
  setMessages: (messages: Message[]) => void;
}

export const useChatStore = create<ChatStore>((set) => ({
  messages: [],
  isStreaming: false,
  
  addMessage: (message) =>
    set((state) => ({
      messages: [
        ...state.messages,
        {
          ...message,
          id: crypto.randomUUID(),
          timestamp: new Date(),
        },
      ],
    })),
  
  startAgentMessage: () =>
    set((state) => ({
      messages: [
        ...state.messages,
        {
          id: crypto.randomUUID(),
          role: 'agent' as const,
          content: '',
          timestamp: new Date(),
        },
      ],
      isStreaming: true,
    })),
  
  appendToLastMessage: (chunk) =>
    set((state) => {
      const messages = [...state.messages];
      const lastMessage = messages[messages.length - 1];
      if (lastMessage && lastMessage.role === 'agent') {
        lastMessage.content += chunk;
      }
      return { messages };
    }),
  
  setStreaming: (streaming) => set({ isStreaming: streaming }),
  
  clearMessages: () => set({ messages: [] }),
  
  setMessages: (messages) => set({ messages }),
}));
