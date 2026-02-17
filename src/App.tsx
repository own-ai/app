import { useEffect, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { Header } from "@/components/layout/Header";
import { MessageList } from "@/components/chat/MessageList";
import { MessageInput } from "@/components/chat/MessageInput";
import { CreateInstanceDialog } from "@/components/instances/CreateInstanceDialog";
import { Settings } from "@/components/settings/Settings";
import { useChatStore } from "@/stores/chatStore";
import { useInstanceStore } from "@/stores/instanceStore";

interface RawMessage {
  id: string;
  role: "user" | "agent" | "system";
  content: string;
  timestamp: string;
  instance_id: string;
}

function App() {
  const { t } = useTranslation();
  const {
    messages,
    isStreaming,
    addMessage,
    startAgentMessage,
    appendToLastMessage,
    setStreaming,
    setMessages,
  } = useChatStore();
  const { instances, activeInstance, loadInstances } = useInstanceStore();
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [isLoadingMessages, setIsLoadingMessages] = useState(false);

  // Load instances on mount
  useEffect(() => {
    loadInstances();
  }, [loadInstances]);

  const loadMessagesForInstance = useCallback(
    async (instanceId: string) => {
      setIsLoadingMessages(true);
      try {
        const loadedMessages = await invoke<RawMessage[]>("load_messages", {
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
        console.error("Failed to load messages:", error);
      } finally {
        setIsLoadingMessages(false);
      }
    },
    [setMessages],
  );

  // Load messages when active instance changes
  useEffect(() => {
    if (activeInstance) {
      loadMessagesForInstance(activeInstance.id);
    }
  }, [activeInstance, loadMessagesForInstance]);

  // Show create dialog if no instances exist
  useEffect(() => {
    if (instances.length === 0 && !activeInstance) {
      setShowCreateDialog(true);
    }
  }, [instances.length, activeInstance]);

  const handleSend = useCallback(
    async (content: string) => {
      if (!activeInstance) return;

      // Add user message to UI (backend will save it)
      addMessage({
        role: "user" as const,
        content,
      });

      // Start empty agent message for streaming
      startAgentMessage();

      let unlisten: UnlistenFn | null = null;

      try {
        // Listen for streaming tokens
        unlisten = await listen<string>("agent:token", (event) => {
          appendToLastMessage(event.payload);
        });

        // Call streaming endpoint (backend saves both messages)
        await invoke("stream_message", {
          request: {
            instance_id: activeInstance.id,
            content,
          },
        });
      } catch (error) {
        console.error("Failed to get response:", error);
        // Keep partial response visible, add error as system message
        addMessage({
          role: "system" as const,
          content: t("chat.streaming_error", { error: String(error) }),
        });
      } finally {
        // Clean up listener
        if (unlisten) {
          unlisten();
        }
        setStreaming(false);
      }
    },
    [
      activeInstance,
      addMessage,
      startAgentMessage,
      appendToLastMessage,
      setStreaming,
      t,
    ],
  );

  const handleOpenSettings = () => {
    setShowSettings(true);
  };

  if (instances.length === 0 && !activeInstance) {
    return (
      <div className="h-screen flex flex-col bg-background">
        <Header onSettingsClick={handleOpenSettings} />
        <div className="flex-1 flex items-center justify-center">
          <div className="text-center">
            <h2 className="text-2xl font-serif mb-4">
              {t("ai_instances.welcome_title")}
            </h2>
            <p className="text-muted mb-6">
              {t("ai_instances.welcome_subtitle")}
            </p>
          </div>
        </div>
        <CreateInstanceDialog
          isOpen={showCreateDialog}
          onClose={() => setShowCreateDialog(false)}
          onOpenSettings={handleOpenSettings}
        />
        <Settings
          isOpen={showSettings}
          onClose={() => setShowSettings(false)}
        />
      </div>
    );
  }

  return (
    <div className="h-screen flex flex-col bg-background">
      <Header onSettingsClick={handleOpenSettings} />

      <main className="flex-1 flex flex-col overflow-hidden">
        {isLoadingMessages ? (
          <div className="flex-1 flex items-center justify-center text-muted">
            <p>{t("chat.loading_messages")}</p>
          </div>
        ) : (
          <MessageList messages={messages} isStreaming={isStreaming} />
        )}
      </main>

      <MessageInput
        onSend={handleSend}
        disabled={!activeInstance || isStreaming}
      />

      <CreateInstanceDialog
        isOpen={showCreateDialog}
        onClose={() => setShowCreateDialog(false)}
        onOpenSettings={handleOpenSettings}
      />
      <Settings isOpen={showSettings} onClose={() => setShowSettings(false)} />
    </div>
  );
}

export default App;
