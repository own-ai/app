import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import {
  AIInstance,
  CreateInstanceRequest,
  ProviderInfo,
  ProviderType,
} from "@/types";

interface InstanceStore {
  instances: AIInstance[];
  activeInstance: AIInstance | null;
  providers: ProviderInfo[];
  isLoading: boolean;

  // Instance Actions
  loadInstances: () => Promise<void>;
  createInstance: (request: CreateInstanceRequest) => Promise<void>;
  switchInstance: (id: string) => Promise<void>;
  deleteInstance: (id: string) => Promise<void>;

  // Provider & API Key Actions
  loadProviders: () => Promise<void>;
  saveApiKey: (provider: ProviderType, apiKey: string) => Promise<void>;
  deleteApiKey: (provider: ProviderType) => Promise<void>;
  hasApiKey: (provider: ProviderType) => Promise<boolean>;
}

export const useInstanceStore = create<InstanceStore>((set, get) => ({
  instances: [],
  activeInstance: null,
  providers: [],
  isLoading: false,

  loadInstances: async () => {
    set({ isLoading: true });
    try {
      const instances = await invoke<AIInstance[]>("list_ai_instances");
      set({ instances });

      // Load active instance
      const active = await invoke<AIInstance | null>("get_active_instance");
      if (active) {
        set({ activeInstance: active });
      } else if (instances.length > 0) {
        // Set first instance as active if none is set
        await get().switchInstance(instances[0].id);
      }
    } catch (error) {
      console.error("Failed to load instances:", error);
    } finally {
      set({ isLoading: false });
    }
  },

  createInstance: async (request: CreateInstanceRequest) => {
    set({ isLoading: true });
    try {
      const instance = await invoke<AIInstance>("create_ai_instance", {
        request,
      });

      set((state) => ({
        instances: [...state.instances, instance],
        activeInstance: instance,
      }));
    } catch (error) {
      console.error("Failed to create instance:", error);
      throw error;
    } finally {
      set({ isLoading: false });
    }
  },

  switchInstance: async (id: string) => {
    try {
      await invoke("set_active_instance", { instanceId: id });
      const instance = get().instances.find((i) => i.id === id);
      set({ activeInstance: instance || null });
    } catch (error) {
      console.error("Failed to switch instance:", error);
      throw error;
    }
  },

  deleteInstance: async (id: string) => {
    set({ isLoading: true });
    try {
      await invoke("delete_ai_instance", { instanceId: id });

      set((state) => {
        const newInstances = state.instances.filter((i) => i.id !== id);
        const newActive =
          state.activeInstance?.id === id ? null : state.activeInstance;

        return {
          instances: newInstances,
          activeInstance: newActive,
        };
      });

      // Set first instance as active if we deleted the active one
      const { instances, activeInstance } = get();
      if (!activeInstance && instances.length > 0) {
        await get().switchInstance(instances[0].id);
      }
    } catch (error) {
      console.error("Failed to delete instance:", error);
      throw error;
    } finally {
      set({ isLoading: false });
    }
  },

  loadProviders: async () => {
    try {
      const providers = await invoke<ProviderInfo[]>("get_providers");
      set({ providers });
    } catch (error) {
      console.error("Failed to load providers:", error);
    }
  },

  saveApiKey: async (provider: ProviderType, apiKey: string) => {
    try {
      await invoke("save_api_key", { provider, apiKey });
      // Reload providers to update has_api_key status
      await get().loadProviders();
    } catch (error) {
      console.error("Failed to save API key:", error);
      throw error;
    }
  },

  deleteApiKey: async (provider: ProviderType) => {
    try {
      await invoke("delete_api_key", { provider });
      // Reload providers to update has_api_key status
      await get().loadProviders();
    } catch (error) {
      console.error("Failed to delete API key:", error);
      throw error;
    }
  },

  hasApiKey: async (provider: ProviderType) => {
    try {
      return await invoke<boolean>("has_api_key", { provider });
    } catch (error) {
      console.error("Failed to check API key:", error);
      return false;
    }
  },
}));
