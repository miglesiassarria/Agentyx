/// Workspace state — Svelte 5 runes-based global store.
///
/// One singleton instance for the app. The store owns the list of
/// workspaces known to Agentyx, the currently selected workspace,
/// the active venv detection result, the file tree cache, and the
/// extra paths list.
///
/// All mutations go through methods (never direct mutation of the
/// state objects from components). Components consume the `$state`
/// proxies reactively.
///
/// See `../../../specs/features/F02-multi-workspace.md` for the
/// end-to-end flow. See `lib/ipc.ts` for the IPC primitives.

import { open as openDialog } from '@tauri-apps/plugin-dialog';

import { events, workspace as workspaceIpc } from '$lib/ipc';
import type { ExtraPathDto, FileEntryDto, VenvSpec, WorkspaceDto } from '$lib/ipc-types';

interface TreeNode {
  /** Absolute canonical path (used as the cache key). */
  path: string;
  /** Display name (basename). */
  name: string;
  /** Whether this is a directory. */
  isDir: boolean;
  /** Whether the directory is currently expanded. */
  expanded: boolean;
  /** True once the directory has been loaded at least once. */
  loaded: boolean;
  /** True while a `list_dir` call is in flight for this node. */
  loading: boolean;
  /** Children when `isDir && loaded`. Empty for files or empty dirs. */
  children: TreeNode[];
  /** Error message if the last load failed. */
  error: string | null;
}

const DEFAULT_IGNORE = [
  '.git',
  'node_modules',
  'target',
  '__pycache__',
  '.venv',
  'venv',
  'dist',
  'build',
  '.next',
  '.cache',
] as const;

class WorkspaceStore {
  list = $state<WorkspaceDto[]>([]);
  selectedId = $state<string | null>(null);
  loadingList = $state(false);
  mutating = $state(false);
  /** Last error message surfaced from any workspace IPC call. */
  lastError = $state<string | null>(null);

  selected = $derived<WorkspaceDto | null>(
    this.selectedId === null ? null : (this.list.find((w) => w.id === this.selectedId) ?? null),
  );

  venv = $state<VenvSpec | null>(null);
  venvLoading = $state(false);
  /** File tree root for the selected workspace (root path → children). */
  fileTree = $state<Record<string, TreeNode>>({});
  fileTreeLoading = $state(false);

  private unlisteners: Array<() => void> = [];

  constructor() {
    // Reactive auto-clear: if a workspace is removed from the list
    // (e.g. deleted), the selection is invalidated.
    $effect.root(() => {
      $effect(() => {
        if (this.selectedId !== null && !this.list.some((w) => w.id === this.selectedId)) {
          this.selectedId = null;
          this.venv = null;
          this.fileTree = {};
        }
      });
    });
  }

  /// Lifecycle

  /// Subscribe to `workspace.*.v1` events so the UI reflects backend
  /// mutations in place (F02.AC18). Must be called once from the
  /// root component on mount; returns an unlisten disposer.
  async attach(): Promise<() => void> {
    const u1 = await events.workspaceExtraPathAdded((p) => {
      this.refreshExtrasFor(p.workspaceId);
    });
    const u2 = await events.workspaceExtraPathRemoved((p) => {
      this.refreshExtrasFor(p.workspaceId);
    });
    this.unlisteners.push(u1, u2);
    return () => {
      this.unlisteners.forEach((u) => u());
      this.unlisteners = [];
    };
  }

  /// Actions

  async loadList(): Promise<void> {
    this.loadingList = true;
    this.lastError = null;
    try {
      this.list = await workspaceIpc.list();
    } catch (e) {
      this.lastError = toMessage(e);
      throw e;
    } finally {
      this.loadingList = false;
    }
  }

  async openViaDialog(): Promise<WorkspaceDto | null> {
    this.mutating = true;
    this.lastError = null;
    try {
      const selected = await openDialog({
        directory: true,
        multiple: false,
        title: 'Open workspace',
      });
      if (typeof selected !== 'string') return null;
      const ws = await workspaceIpc.open(selected);
      await this.loadList();
      this.selectedId = ws.id;
      await this.refreshSelectionData();
      return ws;
    } catch (e) {
      this.lastError = toMessage(e);
      throw e;
    } finally {
      this.mutating = false;
    }
  }

  async select(id: string | null): Promise<void> {
    this.selectedId = id;
    this.venv = null;
    this.fileTree = {};
    if (id !== null) {
      await this.refreshSelectionData();
    }
  }

  async refreshSelectionData(): Promise<void> {
    const id = this.selectedId;
    if (id === null) return;
    this.venvLoading = true;
    this.fileTreeLoading = true;
    try {
      const [venv, ws] = await Promise.all([
        workspaceIpc.detectVenv(id).catch(() => null),
        workspaceIpc.get(id),
      ]);
      this.venv = venv;
      this.fileTreeLoading = true;
      // Re-merge latest DTO into the list (extras may have changed).
      this.list = this.list.map((w) => (w.id === id ? ws : w));
      await this.loadRootEntries();
    } catch (e) {
      this.lastError = toMessage(e);
    } finally {
      this.venvLoading = false;
      this.fileTreeLoading = false;
    }
  }

  async deleteSelected(): Promise<void> {
    const id = this.selectedId;
    if (id === null) return;
    this.mutating = true;
    this.lastError = null;
    try {
      await workspaceIpc.delete(id, false);
      const removedId = id;
      this.list = this.list.filter((w) => w.id !== removedId);
      if (this.selectedId === removedId) {
        this.selectedId = null;
        this.venv = null;
        this.fileTree = {};
      }
    } catch (e) {
      this.lastError = toMessage(e);
      throw e;
    } finally {
      this.mutating = false;
    }
  }

  async addExtraPathViaDialog(): Promise<ExtraPathDto | null> {
    const id = this.selectedId;
    if (id === null) return null;
    this.mutating = true;
    this.lastError = null;
    try {
      const picked = await openDialog({
        directory: true,
        multiple: false,
        title: 'Add directory to workspace',
      });
      if (typeof picked !== 'string') return null;
      const extra = await workspaceIpc.addExtraPath(id, picked, null);
      // Optimistic local merge (the event listener would also do it
      // via `refreshExtrasFor`); merging here keeps the UI snappy.
      this.list = this.list.map((w) =>
        w.id === id ? { ...w, extraPaths: [...w.extraPaths, extra] } : w,
      );
      return extra;
    } catch (e) {
      this.lastError = toMessage(e);
      throw e;
    } finally {
      this.mutating = false;
    }
  }

  async removeExtraPath(path: string): Promise<void> {
    const id = this.selectedId;
    if (id === null) return;
    this.mutating = true;
    this.lastError = null;
    try {
      await workspaceIpc.removeExtraPath(id, path);
      this.list = this.list.map((w) =>
        w.id === id ? { ...w, extraPaths: w.extraPaths.filter((e) => e.path !== path) } : w,
      );
    } catch (e) {
      this.lastError = toMessage(e);
      throw e;
    } finally {
      this.mutating = false;
    }
  }

  toggleNode(path: string): void {
    const node = this.fileTree[path];
    if (!node || !node.isDir) return;
    node.expanded = !node.expanded;
    if (node.expanded && !node.loaded) {
      void this.loadChildren(path);
    }
  }

  private async loadRootEntries(): Promise<void> {
    const ws = this.selected;
    if (ws === null) return;
    this.fileTreeLoading = true;
    try {
      const entries = await workspaceIpc.listDir(ws.id, ws.rootPath);
      this.fileTree = {
        ...this.fileTree,
        [ws.rootPath]: {
          path: ws.rootPath,
          name: basename(ws.rootPath),
          isDir: true,
          expanded: true,
          loaded: true,
          loading: false,
          children: entriesToNodes(entries),
          error: null,
        },
      };
    } catch (e) {
      const rootPath = ws.rootPath;
      this.fileTree = {
        ...this.fileTree,
        [rootPath]: {
          path: rootPath,
          name: basename(rootPath),
          isDir: true,
          expanded: true,
          loaded: true,
          loading: false,
          children: [],
          error: toMessage(e),
        },
      };
    } finally {
      this.fileTreeLoading = false;
    }
  }

  private async loadChildren(path: string): Promise<void> {
    const ws = this.selected;
    if (ws === null) return;
    const node = this.fileTree[path];
    if (node === undefined) return;
    if (!shouldShowNode(node.name)) {
      node.children = [];
      node.loaded = true;
      node.loading = false;
      return;
    }
    node.loading = true;
    node.error = null;
    try {
      const entries = await workspaceIpc.listDir(ws.id, path);
      node.children = entriesToNodes(entries);
      node.loaded = true;
    } catch (e) {
      node.error = toMessage(e);
      node.children = [];
    } finally {
      node.loading = false;
    }
  }

  private async refreshExtrasFor(workspaceId: string): Promise<void> {
    if (this.selectedId !== workspaceId) return;
    try {
      const ws = await workspaceIpc.get(workspaceId);
      this.list = this.list.map((w) => (w.id === workspaceId ? ws : w));
    } catch (e) {
      this.lastError = toMessage(e);
    }
  }
}

function basename(p: string): string {
  const idx = Math.max(p.lastIndexOf('/'), p.lastIndexOf('\\'));
  return idx < 0 ? p : p.slice(idx + 1);
}

function shouldShowNode(name: string): boolean {
  return !DEFAULT_IGNORE.includes(name as (typeof DEFAULT_IGNORE)[number]);
}

function entriesToNodes(entries: FileEntryDto[]): TreeNode[] {
  return entries
    .filter((e) => shouldShowNode(e.name))
    .map((e) => ({
      path: e.path,
      name: e.name,
      isDir: e.isDir,
      expanded: false,
      loaded: false,
      loading: false,
      children: [],
      error: null,
    }));
}

function toMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  return String(e);
}

export const workspaceStore = new WorkspaceStore();
export type { TreeNode as FileTreeNodeData };
