import { invoke } from "@tauri-apps/api/core";
import { create } from "zustand";

export interface DatabaseInfo {
  id: string;
  name: string;
  path: string;
  is_active: boolean;
}

interface DatabaseState {
  databases: DatabaseInfo[];
  activeId: string | null;
  loading: boolean;
  error: string | null;
  fetchDatabases: () => Promise<void>;
  createDatabase: (name: string, path: string) => Promise<void>;
  addExistingDatabase: (name: string, path: string) => Promise<void>;
  renameDatabase: (id: string, name: string) => Promise<void>;
  removeRegistration: (id: string) => Promise<void>;
  switchDatabase: (id: string) => Promise<void>;
}

const message = (error: unknown) => String(error);

export const useDatabaseStore = create<DatabaseState>((set) => ({
  databases: [],
  activeId: null,
  loading: false,
  error: null,

  fetchDatabases: async () => {
    set({ loading: true, error: null });
    try {
      const databases = await invoke<DatabaseInfo[]>("list_databases");
      set({ databases, activeId: databases.find((database) => database.is_active)?.id ?? null, loading: false });
    } catch (error) {
      set({ loading: false, error: message(error) });
      throw error;
    }
  },

  createDatabase: async (name, path) => {
    set({ error: null });
    try {
      const database = await invoke<DatabaseInfo>("create_database", { name, path });
      set((state) => ({ databases: [...state.databases, database] }));
    } catch (error) {
      set({ error: message(error) });
      throw error;
    }
  },

  addExistingDatabase: async (name, path) => {
    set({ error: null });
    try {
      const database = await invoke<DatabaseInfo>("add_existing_database", { name, path });
      set((state) => ({ databases: [...state.databases, database] }));
    } catch (error) {
      set({ error: message(error) });
      throw error;
    }
  },

  renameDatabase: async (id, name) => {
    set({ error: null });
    try {
      await invoke("rename_database", { id, name });
      set((state) => ({ databases: state.databases.map((database) => database.id === id ? { ...database, name } : database) }));
    } catch (error) {
      set({ error: message(error) });
      throw error;
    }
  },

  removeRegistration: async (id) => {
    set({ error: null });
    try {
      await invoke("remove_database_registration", { id });
      set((state) => ({ databases: state.databases.filter((database) => database.id !== id) }));
    } catch (error) {
      set({ error: message(error) });
      throw error;
    }
  },

  switchDatabase: async (id) => {
    set({ error: null });
    try {
      await invoke("switch_database", { id });
      set((state) => ({
        activeId: id,
        databases: state.databases.map((database) => ({ ...database, is_active: database.id === id })),
      }));
    } catch (error) {
      set({ error: message(error) });
      throw error;
    }
  },
}));
