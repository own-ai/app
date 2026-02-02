import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useTranslation } from 'react-i18next';
import { Header } from '@/components/layout/Header';
import { MessageList } from '@/components/chat/MessageList';
import { MessageInput } from '@/components/chat/MessageInput';
import { TypingIndicator } from '@/components/chat/TypingIndicator';
import { CreateInstanceDialog } from '@/components/instances/CreateInstanceDialog';
import { useChatStore } from '@/stores/chatStore';
import { useInstanceStore } from '@/stores/instanceStore';

function App() {
  const { t } = useTranslation();
  const { messages, isTyping, addMessage, setTyping, setMessages } = useChatStore();
  const { instances, activeInstance, loadInstances, createInstance } = useInstanceStore();
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [isLoadingMessages, setIsLoadingMessages] = useState(false);

  // Load instances on mount
  useEffect(() => {
    loadInstances();
  }, [loadInstances]);

  // Load messages when active instance changes
  useEffect(() => {
    if (activeInstance) {
      loadMessagesForInstance(activeInstance.id);
    }
  }, [activeInstance?.id]);

  // Show create dialog if no instances exist
  useEffect(() => {
    if (instances.length === 0 && !activeInstance) {
      setShowCreateDialog(true);
    }
  }, [instances.length, activeInstance]);

  const loadMessagesForInstance = async (instanceId: string) => {
    setIsLoadingMessages(true);
    try {
      const loadedMessages = await invoke<any[]>('load_messages', {
        instanceId,
        limit: 1000,
        offset: 0,
      });

      // Convert timestamps to Date objects
      const parsedMessages = loadedMessages.map((msg) => ({
        ...msg,
        timestamp: new Date(msg.timestamp),
      }));

      setMessages(parsedMessages);
    } catch (error) {
      console.error('Failed to load messages:', error);
    } finally {
      setIsLoadingMessages(false);
    }
  };

  const handleSend = async (content: string) => {
    if (!activeInstance) return;

    // Add user message
    const userMessage = {
      role: 'user' as const,
      content,
    };
    addMessage(userMessage);

    // Save user message to database
    try {
      await invoke('save_message', {
        instanceId: activeInstance.id,
        message: {
          id: messages[messages.length]?.id || crypto.randomUUID(),
          role: 'user',
          content,
          timestamp: new Date().toISOString(),
          metadata: null,
        },
      });
    } catch (error) {
      console.error('Failed to save user message:', error);
    }

    // Show typing indicator
    setTyping(true);

    try {
      // Call mock agent (Note: Rust uses snake_case)
      const response = await invoke<any>('send_message_mock', {
        request: {
          instance_id: activeInstance.id,
          content,
        },
      });

      // Add agent message
      addMessage({
        role: 'agent' as const,
        content: response.content,
      });

      // Save agent message to database
      await invoke('save_message', {
        instanceId: activeInstance.id,
        message: {
          ...response,
          timestamp: new Date().toISOString(),
        },
      });
    } catch (error) {
      console.error('Failed to get response:', error);
      addMessage({
        role: 'system' as const,
        content: `Error: ${error}`,
      });
    } finally {
      setTyping(false);
    }
  };

  const handleCreateInstance = async (name: string) => {
    await createInstance(name);
    setShowCreateDialog(false);
  };

  if (instances.length === 0 && !activeInstance) {
    return (
      <div className="h-screen flex flex-col bg-background">
        <Header />
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center">
            <h2 className="text-2xl font-serif mb-4">{t('ai_instances.welcome_title')}</h2>
            <p className="text-muted mb-6">{t('ai_instances.welcome_subtitle')}</p>
          </div>
        </div>
        <CreateInstanceDialog
          isOpen={showCreateDialog}
          onClose={() => setShowCreateDialog(false)}
          onCreate={handleCreateInstance}
        />
      </div>
    );
  }

  return (
    <div className="h-screen flex flex-col bg-background">
      <Header />
      
      <main className="flex-1 flex flex-col overflow-hidden">
        {isLoadingMessages ? (
          <div className="flex-1 flex items-center justify-center text-muted">
            <p>{t('chat.loading_messages')}</p>
          </div>
        ) : (
          <MessageList messages={messages} />
        )}
        
        {isTyping && <TypingIndicator />}
      </main>
      
      <MessageInput onSend={handleSend} disabled={!activeInstance || isTyping} />
    </div>
  );
}

export default App;
