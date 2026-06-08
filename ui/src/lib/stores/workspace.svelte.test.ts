/// WorkspaceStore unit tests — covers F02 flows plus the
/// F06.AC4/AC5 browser-safe path prompt flow.
///
/// We stub the IPC layer and verify:
/// - `openViaPath` (programmatic) calls `workspace.open` and
///   updates the store's `selectedId`.
/// - `addExtraPathViaPath` (programmatic) calls
///   `workspace.addExtraPath` and merges optimistically.
/// - In browser mode, `openViaDialog` triggers the path-prompt
///   store and only calls `workspace.open` after the user
///   submits an absolute path.
/// - In browser mode, `addExtraPathViaDialog` does the same for
///   extra paths.
/// - Cancelling the path-prompt (browser) does not call IPC.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import type { ExtraPathDto, FileEntryDto, WorkspaceDto } from '$lib/ipc-types';

const stub = vi.hoisted(() => ({
  workspaceList: vi.fn(),
  workspaceOpen: vi.fn(),
  workspaceGet: vi.fn(),
  workspaceDelete: vi.fn(),
  workspaceDetectVenv: vi.fn(),
  workspaceAddExtraPath: vi.fn(),
  workspaceRemoveExtraPath: vi.fn(),
  workspaceEffectivePaths: vi.fn(),
  workspaceListDir: vi.fn(),
  lastExtraPathAddedHandler: null as ((p: unknown) => void) | null,
  lastExtraPathRemovedHandler: null as ((p: unknown) => void) | null,
}));

vi.mock('$lib/ipc', () => ({
  isBrowserMode: () => true,
  events: {
    workspaceExtraPathAdded: async (cb: (p: unknown) => void): Promise<() => void> => {
      stub.lastExtraPathAddedHandler = cb;
      return () => {
        if (stub.lastExtraPathAddedHandler === cb) {
          stub.lastExtraPathAddedHandler = null;
        }
      };
    },
    workspaceExtraPathRemoved: async (cb: (p: unknown) => void): Promise<() => void> => {
      stub.lastExtraPathRemovedHandler = cb;
      return () => {
        if (stub.lastExtraPathRemovedHandler === cb) {
          stub.lastExtraPathRemovedHandler = null;
        }
      };
    },
  },
  workspace: {
    list: stub.workspaceList,
    open: stub.workspaceOpen,
    get: stub.workspaceGet,
    delete: stub.workspaceDelete,
    detectVenv: stub.workspaceDetectVenv,
    addExtraPath: stub.workspaceAddExtraPath,
    removeExtraPath: stub.workspaceRemoveExtraPath,
    effectivePaths: stub.workspaceEffectivePaths,
    listDir: stub.workspaceListDir,
  },
}));

// We force the path-prompt store to be re-imported alongside the
// workspace store (they share the mocked $lib/ipc, so this is safe).
import { pathPromptStore } from '$lib/stores/path-prompt.svelte';
import { workspaceStore } from '$lib/stores/workspace.svelte';

const WS_ID = '01HWS0000000000000000000A0';
const WS_ID_2 = '01HWS0000000000000000000A1';

function makeWs(overrides: Partial<WorkspaceDto> = {}): WorkspaceDto {
  return {
    id: WS_ID,
    name: 'demo',
    rootPath: '/tmp/demo',
    extraPaths: [],
    hasVenv: false,
    ...overrides,
  };
}

function makeExtra(path: string, label: string): ExtraPathDto {
  return { id: `extra-${path}`, path, label, addedAt: 1700000000000 };
}

function makeFileEntry(name: string, isDir: boolean): FileEntryDto {
  return {
    name,
    path: `/tmp/demo/${name}`,
    isDir,
    isSymlink: false,
    size: 0,
    modifiedAt: 1700000000000,
  };
}

beforeEach(() => {
  stub.workspaceList.mockReset();
  stub.workspaceOpen.mockReset();
  stub.workspaceGet.mockReset();
  stub.workspaceDelete.mockReset();
  stub.workspaceDetectVenv.mockReset();
  stub.workspaceAddExtraPath.mockReset();
  stub.workspaceRemoveExtraPath.mockReset();
  stub.workspaceEffectivePaths.mockReset();
  stub.workspaceListDir.mockReset();
  stub.lastExtraPathAddedHandler = null;
  stub.lastExtraPathRemovedHandler = null;

  // Force the store into a clean state between tests.
  workspaceStore.list = [];
  workspaceStore.selectedId = null;
  workspaceStore.venv = null;
  workspaceStore.fileTree = {};
  workspaceStore.lastError = null;
  // Reset path-prompt pending state.
  if (pathPromptStore.pending !== null) {
    pathPromptStore.cancel();
  }
});

afterEach(() => {
  if (pathPromptStore.pending !== null) {
    pathPromptStore.cancel();
  }
});

describe('WorkspaceStore — programmatic open/add', () => {
  it('openViaPath calls workspace.open and updates selectedId', async () => {
    const created = makeWs({ id: WS_ID_2, rootPath: '/srv/proj' });
    stub.workspaceOpen.mockResolvedValueOnce(created);
    stub.workspaceList.mockResolvedValueOnce([created]);
    stub.workspaceDetectVenv.mockResolvedValueOnce(null);
    stub.workspaceGet.mockResolvedValueOnce(created);
    stub.workspaceListDir.mockResolvedValueOnce([]);

    const ws = await workspaceStore.openViaPath('/srv/proj');

    expect(stub.workspaceOpen).toHaveBeenCalledWith('/srv/proj', undefined);
    expect(ws.id).toBe(WS_ID_2);
    expect(workspaceStore.selectedId).toBe(WS_ID_2);
    expect(workspaceStore.list).toHaveLength(1);
  });

  it('addExtraPathViaPath merges the new extra into the list', async () => {
    const ws = makeWs();
    workspaceStore.list = [ws];
    workspaceStore.selectedId = ws.id;
    const extra = makeExtra('/srv/lib', 'lib');
    stub.workspaceAddExtraPath.mockResolvedValueOnce(extra);

    const result = await workspaceStore.addExtraPathViaPath('/srv/lib', 'lib');

    expect(stub.workspaceAddExtraPath).toHaveBeenCalledWith(ws.id, '/srv/lib', 'lib');
    expect(result).toEqual(extra);
    expect(workspaceStore.list[0]?.extraPaths).toEqual([extra]);
  });

  it('addExtraPathViaPath throws when no workspace is selected', async () => {
    workspaceStore.selectedId = null;
    await expect(workspaceStore.addExtraPathViaPath('/srv/lib', null)).rejects.toThrow(
      /no workspace selected/i,
    );
    expect(stub.workspaceAddExtraPath).not.toHaveBeenCalled();
  });
});

describe('F06.AC4/AC5 — browser-safe path prompt', () => {
  it('openViaDialog in browser mode waits for the path prompt before calling IPC', async () => {
    const created = makeWs({ id: WS_ID_2, rootPath: '/srv/typed' });
    stub.workspaceList.mockResolvedValueOnce([created]);
    stub.workspaceDetectVenv.mockResolvedValueOnce(null);
    stub.workspaceGet.mockResolvedValueOnce(created);
    stub.workspaceListDir.mockResolvedValueOnce([]);
    stub.workspaceOpen.mockResolvedValueOnce(created);

    const inFlight = workspaceStore.openViaDialog();
    // Give the microtask queue a chance to enter the prompt request.
    await Promise.resolve();
    await Promise.resolve();
    expect(pathPromptStore.pending).not.toBeNull();
    expect(stub.workspaceOpen).not.toHaveBeenCalled();

    pathPromptStore.submit({ path: '/srv/typed', label: 'typed' });
    const result = await inFlight;

    expect(stub.workspaceOpen).toHaveBeenCalledWith('/srv/typed', undefined);
    expect(result?.id).toBe(WS_ID_2);
    expect(pathPromptStore.pending).toBeNull();
  });

  it('openViaDialog returns null and does not call IPC when the user cancels', async () => {
    const inFlight = workspaceStore.openViaDialog();
    await Promise.resolve();
    await Promise.resolve();
    expect(pathPromptStore.pending).not.toBeNull();
    pathPromptStore.cancel();
    const result = await inFlight;

    expect(result).toBeNull();
    expect(stub.workspaceOpen).not.toHaveBeenCalled();
  });

  it('addExtraPathViaDialog in browser mode uses the path prompt and forwards the label', async () => {
    const ws = makeWs();
    workspaceStore.list = [ws];
    workspaceStore.selectedId = ws.id;
    const extra = makeExtra('/srv/lib', 'lib');
    stub.workspaceAddExtraPath.mockResolvedValueOnce(extra);

    const inFlight = workspaceStore.addExtraPathViaDialog();
    await Promise.resolve();
    await Promise.resolve();
    expect(pathPromptStore.pending).not.toBeNull();

    pathPromptStore.submit({ path: '/srv/lib', label: 'lib' });
    const result = await inFlight;

    expect(stub.workspaceAddExtraPath).toHaveBeenCalledWith(ws.id, '/srv/lib', 'lib');
    expect(result).toEqual(extra);
  });

  it('addExtraPathViaDialog returns null when no workspace is selected', async () => {
    workspaceStore.selectedId = null;
    const result = await workspaceStore.addExtraPathViaDialog();
    expect(result).toBeNull();
    expect(pathPromptStore.pending).toBeNull();
    expect(stub.workspaceAddExtraPath).not.toHaveBeenCalled();
  });
});

describe('PathPromptStore — validation', () => {
  it('rejects a second concurrent request until the first is resolved', async () => {
    const first = pathPromptStore.requestWorkspaceOpen();
    await Promise.resolve();
    const second = await pathPromptStore.requestWorkspaceOpen();
    expect(second).toBeNull();
    pathPromptStore.cancel();
    await first;
  });
});

describe('File tree refresh after selection', () => {
  it('loadRootEntries populates the root tree node from listDir', async () => {
    const ws = makeWs();
    workspaceStore.list = [ws];
    workspaceStore.selectedId = ws.id;
    stub.workspaceDetectVenv.mockResolvedValueOnce(null);
    stub.workspaceGet.mockResolvedValueOnce(ws);
    stub.workspaceListDir.mockResolvedValueOnce([makeFileEntry('src', true)]);

    await workspaceStore.refreshSelectionData();

    const root = workspaceStore.fileTree[ws.rootPath];
    expect(root).toBeDefined();
    expect(root?.isDir).toBe(true);
    expect(root?.loaded).toBe(true);
    expect(root?.children.map((c) => c.name)).toEqual(['src']);
  });
});
