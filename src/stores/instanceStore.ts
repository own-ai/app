import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';
import { AIInstance, CreateInstanceRequest } from '@/types';

interface InstanceStore {
  instances: AIInstance[];
  activeInstance: AIInstance | null;
  isLoading: boolean;
  
  // Actions
  loadInstances: () => Promise<void>;
  createInstance: (name: string) => Promise<void>;
  switchInstance: (id: string) => Promise<void>;
  deleteInstance: (id: string) => Promise<void>;
}

export const useInstanceStore = create<InstanceStore>((set, get) => ({
  instances: [],
  activeInstance: null,
  isLoading: false,
  
  loadInstances: async () => {
    set({ isLoading: true });
    try {
      const instances = await invoke<AIInstance[]>('list_ai_instances');
      set({ instances });
      
      // Load active instance
      const active = await invoke<AIInstance | null>('get_active_instance');
      if (active) {
        set({ activeInstance: active });
      } else if (instances.length > 0) {
        // Set first instance as active if none is set
        await get().switchInstance(instances[0].id);
      }
    } catch (error) {
      console.error('Failed to load instances:', error);
    } finally {
      set({ isLoading: false });
    }
  },
  
  createInstance: async (name: string) => {
    set({ isLoading: true });
    try {
      const request: CreateInstanceRequest = { name };
      const instance = await invoke<AIInstance>('create_ai_instance', { request });
      
      set((state) => ({
        instances: [...state.instances, instance],
        activeInstance: instance,
      }));
    } catch (error) {
      console.error('Failed to create instance:', error);
      throw error;
    } finally {
      set({ isLoading: false });
    }
  },
  
  switchInstance: async (id: string) => {
    try {
      await invoke('set_active_instance', { instanceId: id });
      const instance = get().instances.find((i) => i.id === id);
      set({ activeInstance: instance || null });
    } catch (error) {
      console.error('Failed to switch instance:', error);
      throw error;
    }
  },
  
  deleteInstance: async (id: string) => {
    set({ isLoading: true });
    try {
      await invoke('delete_ai_instance', { instanceId: id });
      
      set((state) => {
        const newInstances = state.instances.filter((i) => i.id !== id);
        const newActive = state.activeInstance?.id === id ? null : state.activeInstance;
        
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
      console.error('Failed to delete instance:', error);
      throw error;
    } finally {
      set({ isLoading: false });
    }
  },
}));
