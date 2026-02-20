import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import { Program, CanvasViewMode } from "@/types";

interface CanvasStore {
  programs: Program[];
  activeProgram: Program | null;
  programUrl: string | null;
  viewMode: CanvasViewMode;
  isLoading: boolean;

  // Actions
  loadPrograms: (instanceId: string) => Promise<void>;
  selectProgram: (instanceId: string, programName: string) => Promise<void>;
  closeProgram: () => void;
  deleteProgram: (instanceId: string, programName: string) => Promise<void>;
  setViewMode: (mode: CanvasViewMode) => void;
  clearCanvas: () => void;
  refreshActiveProgram: (newVersion?: string) => void;
}

export const useCanvasStore = create<CanvasStore>((set, get) => ({
  programs: [],
  activeProgram: null,
  programUrl: null,
  viewMode: "chat",
  isLoading: false,

  loadPrograms: async (instanceId: string) => {
    try {
      const programs = await invoke<Program[]>("list_programs", {
        instanceId,
      });
      set({ programs });
    } catch (error) {
      console.error("Failed to load programs:", error);
    }
  },

  selectProgram: async (instanceId: string, programName: string) => {
    set({ isLoading: true });
    try {
      const url = await invoke<string>("get_program_url", {
        instanceId,
        programName,
      });
      const program =
        get().programs.find((p) => p.name === programName) || null;
      set({
        activeProgram: program,
        programUrl: url,
        isLoading: false,
      });
    } catch (error) {
      console.error("Failed to get program URL:", error);
      set({ isLoading: false });
    }
  },

  closeProgram: () => {
    set({
      activeProgram: null,
      programUrl: null,
    });
  },

  deleteProgram: async (instanceId: string, programName: string) => {
    try {
      await invoke("delete_program", {
        instanceId,
        programName,
      });

      const { activeProgram } = get();

      // If the deleted program was active, close it
      if (activeProgram?.name === programName) {
        set({ activeProgram: null, programUrl: null });
      }

      // Reload programs list
      await get().loadPrograms(instanceId);

      // If no programs left, switch back to chat view
      if (get().programs.length === 0) {
        set({ viewMode: "chat" });
      }
    } catch (error) {
      console.error("Failed to delete program:", error);
      throw error;
    }
  },

  setViewMode: (mode: CanvasViewMode) => {
    set({ viewMode: mode });
  },

  clearCanvas: () => {
    set({
      programs: [],
      activeProgram: null,
      programUrl: null,
      viewMode: "chat",
      isLoading: false,
    });
  },

  refreshActiveProgram: (newVersion?: string) => {
    const { activeProgram, programUrl } = get();
    if (!activeProgram || !programUrl) return;

    // Update version if provided
    const updatedProgram = newVersion
      ? { ...activeProgram, version: newVersion }
      : activeProgram;

    // Cache-bust iframe by appending timestamp query parameter
    const baseUrl = programUrl.split("?")[0];
    const freshUrl = `${baseUrl}?v=${Date.now()}`;

    set({
      activeProgram: updatedProgram,
      programUrl: freshUrl,
    });
  },
}));
