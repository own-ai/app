import { useEffect, useState, useCallback, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { Header } from "@/components/layout/Header";
import { MessageList } from "@/components/chat/MessageList";
import { MessageInput } from "@/components/chat/MessageInput";
import { CreateInstanceDialog } from "@/components/instances/CreateInstanceDialog";
import { Settings } from "@/components/settings/Settings";
import { CanvasPanel } from "@/components/canvas/CanvasPanel";
import { useChatStore } from "@/stores/chatStore";
import { useInstanceStore } from "@/stores/instanceStore";
import { useCanvasStore } from "@/stores/canvasStore";
import { cn } from "@/utils/cn";

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
  const {
    programs,
    viewMode,
    loadPrograms,
    selectProgram,
    setViewMode,
    clearCanvas,
    refreshActiveProgram,
  } = useCanvasStore();
  const [showCreateDialog, setShowCreateDialog] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [isLoadingMessages, setIsLoadingMessages] = useState(false);

  // Track previous program count for auto-detection
  const prevProgramCountRef = useRef(0);

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

  // Load messages and programs when active instance changes
  useEffect(() => {
    if (activeInstance) {
      loadMessagesForInstance(activeInstance.id);
      loadPrograms(activeInstance.id);
    } else {
      clearCanvas();
    }
  }, [activeInstance, loadMessagesForInstance, loadPrograms, clearCanvas]);

  // Update prevProgramCountRef when programs change (not on first load)
  useEffect(() => {
    prevProgramCountRef.current = programs.length;
  }, [programs.length]);

  // Listen for canvas:open_program events from the backend
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    const setup = async () => {
      unlisten = await listen<{ program_name: string }>(
        "canvas:open_program",
        async (event) => {
          if (!activeInstance) return;
          // Reload programs to ensure list is fresh
          await loadPrograms(activeInstance.id);
          // Open the program in split view
          setViewMode("split");
          await selectProgram(activeInstance.id, event.payload.program_name);
        },
      );
    };

    setup();
    return () => {
      if (unlisten) unlisten();
    };
  }, [activeInstance, loadPrograms, setViewMode, selectProgram]);

  // Listen for canvas:program_updated events from the backend (auto-reload)
  useEffect(() => {
    let unlisten: UnlistenFn | null = null;

    const setup = async () => {
      unlisten = await listen<{ program_name: string; version: string }>(
        "canvas:program_updated",
        async (event) => {
          const state = useCanvasStore.getState();
          // Only refresh if the updated program is currently displayed
          if (state.activeProgram?.name === event.payload.program_name) {
            refreshActiveProgram(event.payload.version);
          }
          // Reload programs list to update version info
          if (activeInstance) {
            await loadPrograms(activeInstance.id);
          }
        },
      );
    };

    setup();
    return () => {
      if (unlisten) unlisten();
    };
  }, [activeInstance, loadPrograms, refreshActiveProgram]);

  // Show create dialog if no instances exist
  useEffect(() => {
    if (instances.length === 0 && !activeInstance) {
      setShowCreateDialog(true);
    }
  }, [instances.length, activeInstance]);

  // Auto-detect new programs after streaming completes
  const checkForNewPrograms = useCallback(async () => {
    if (!activeInstance) return;

    const prevCount = prevProgramCountRef.current;
    await loadPrograms(activeInstance.id);
    const currentPrograms = useCanvasStore.getState().programs;

    // If a new program appeared, auto-open it in split view
    if (currentPrograms.length > prevCount && currentPrograms.length > 0) {
      // Find the newest program (last in list by updated_at)
      const newest = [...currentPrograms].sort(
        (a, b) =>
          new Date(b.updated_at).getTime() - new Date(a.updated_at).getTime(),
      )[0];

      if (newest) {
        setViewMode("split");
        await selectProgram(activeInstance.id, newest.name);
      }
    }
  }, [activeInstance, loadPrograms, setViewMode, selectProgram]);

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

        // Check if the agent created any new programs
        await checkForNewPrograms();
      }
    },
    [
      activeInstance,
      addMessage,
      startAgentMessage,
      appendToLastMessage,
      setStreaming,
      checkForNewPrograms,
      t,
    ],
  );

  const handleOpenSettings = () => {
    setShowSettings(true);
  };

  const handleCanvasToggle = () => {
    if (viewMode === "chat") {
      setViewMode("split");
    } else {
      setViewMode("chat");
    }
  };

  const handleCanvasClose = () => {
    setViewMode("chat");
  };

  const isCanvasVisible = viewMode === "split" || viewMode === "canvas";

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
      <Header
        onSettingsClick={handleOpenSettings}
        onCanvasToggle={handleCanvasToggle}
        isCanvasOpen={isCanvasVisible}
        hasPrograms={programs.length > 0}
      />

      <div className="flex-1 flex overflow-hidden">
        {/* Chat panel */}
        {viewMode !== "canvas" && (
          <div
            className={cn("flex flex-col overflow-hidden", "flex-1 min-w-0")}
          >
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
          </div>
        )}

        {/* Canvas panel */}
        {isCanvasVisible && activeInstance && (
          <div
            className={cn(
              "flex flex-col overflow-hidden",
              viewMode === "split" && "w-1/2 min-w-80",
              viewMode === "canvas" && "flex-1",
            )}
          >
            <CanvasPanel
              instanceId={activeInstance.id}
              programs={programs}
              onClose={handleCanvasClose}
            />
          </div>
        )}
      </div>

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
