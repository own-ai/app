import { useCallback, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { X, Maximize2, Minimize2 } from "lucide-react";
import { cn } from "@/utils/cn";
import { IconButton } from "@/components/ui/IconButton";
import { ProgramList } from "./ProgramList";
import { useCanvasStore } from "@/stores/canvasStore";
import { Program } from "@/types";

interface BridgeResponse {
  success: boolean;
  data?: unknown;
  error?: string;
}

interface CanvasPanelProps {
  instanceId: string;
  programs: Program[];
  onClose: () => void;
}

export const CanvasPanel = ({
  instanceId,
  programs,
  onClose,
}: CanvasPanelProps) => {
  const { t } = useTranslation();
  const {
    activeProgram,
    programUrl,
    isLoading,
    viewMode,
    selectProgram,
    closeProgram,
    deleteProgram,
    setViewMode,
  } = useCanvasStore();

  const handleSelectProgram = useCallback(
    (programName: string) => {
      selectProgram(instanceId, programName);
    },
    [instanceId, selectProgram],
  );

  const handleDeleteProgram = useCallback(
    async (programName: string) => {
      await deleteProgram(instanceId, programName);
    },
    [instanceId, deleteProgram],
  );

  const handleToggleFullscreen = useCallback(() => {
    setViewMode(viewMode === "canvas" ? "split" : "canvas");
  }, [viewMode, setViewMode]);

  // Ref for the iframe element (used for postMessage responses)
  const iframeRef = useRef<HTMLIFrameElement>(null);

  // Bridge API: Listen for postMessage requests from the Canvas iframe
  useEffect(() => {
    const handleMessage = async (event: MessageEvent) => {
      if (
        !event.data ||
        event.data.type !== "ownai-bridge-request" ||
        !activeProgram
      ) {
        return;
      }

      const { requestId, method, params } = event.data;

      try {
        const response = await invoke<BridgeResponse>("bridge_request", {
          instanceId,
          programName: activeProgram.name,
          method,
          params: params || {},
        });

        // Send response back to the iframe
        iframeRef.current?.contentWindow?.postMessage(
          {
            type: "ownai-bridge-response",
            requestId,
            success: response.success,
            data: response.data,
            error: response.error,
          },
          "*",
        );
      } catch (error) {
        // Send error response back to the iframe
        iframeRef.current?.contentWindow?.postMessage(
          {
            type: "ownai-bridge-response",
            requestId,
            success: false,
            error: String(error),
          },
          "*",
        );
      }
    };

    window.addEventListener("message", handleMessage);
    return () => window.removeEventListener("message", handleMessage);
  }, [instanceId, activeProgram]);

  return (
    <div className="flex flex-col h-full bg-background border-l border-border">
      {/* Toolbar */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-background shrink-0">
        <div className="flex items-center gap-2 min-w-0">
          {activeProgram ? (
            <>
              <span className="font-sans font-medium text-foreground truncate">
                {activeProgram.name}
              </span>
              <span className="text-xs font-mono text-muted shrink-0">
                {t("canvas.version", { version: activeProgram.version })}
              </span>
            </>
          ) : (
            <span className="font-sans text-sm text-muted">
              {t("canvas.programs")}
            </span>
          )}
        </div>

        <div className="flex items-center gap-1 shrink-0">
          {activeProgram && (
            <>
              <IconButton
                icon={viewMode === "canvas" ? Minimize2 : Maximize2}
                label={
                  viewMode === "canvas"
                    ? t("canvas.exit_fullscreen")
                    : t("canvas.fullscreen")
                }
                onClick={handleToggleFullscreen}
              />
              <IconButton
                icon={X}
                label={t("canvas.close_canvas")}
                onClick={closeProgram}
              />
            </>
          )}
          {!activeProgram && (
            <IconButton
              icon={X}
              label={t("canvas.close_canvas")}
              onClick={onClose}
            />
          )}
        </div>
      </div>

      {/* Content area */}
      {isLoading ? (
        <div className="flex-1 flex items-center justify-center text-muted">
          <p className="font-sans text-sm">{t("canvas.loading_program")}</p>
        </div>
      ) : activeProgram && programUrl ? (
        <iframe
          ref={iframeRef}
          src={programUrl}
          title={activeProgram.name}
          className={cn("flex-1 w-full border-0 bg-white")}
          sandbox="allow-scripts allow-forms allow-modals allow-same-origin"
        />
      ) : (
        <ProgramList
          programs={programs}
          onSelect={handleSelectProgram}
          onDelete={handleDeleteProgram}
        />
      )}
    </div>
  );
};
