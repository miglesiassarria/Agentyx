/// Path prompt store — in-app modal for absolute path input.
///
/// Browser mode cannot use `@tauri-apps/plugin-dialog` because no
/// native dialog is reachable. The store owns a single `request`
/// queue: components call `requestWorkspaceOpen()` or
/// `requestExtraPath()`, the host component (mounted in
/// `app.svelte`) renders `PathPromptDialog`, and the promise
/// resolves with the typed value (or `null` on cancel).
///
/// Tauri mode bypasses this store entirely and calls the dialog
/// plugin directly from the workspace store.
///
/// See `specs/features/F06-web-server-lan.md` (F06.AC4/AC5) and
/// `specs/features/F02-multi-workspace.md` §Extras for context.

export type PathPromptKind = 'open_workspace' | 'add_extra_path';

export interface PathPromptRequest {
  kind: PathPromptKind;
  title: string;
  /** Short instruction shown above the input. */
  hint: string;
  /** True when an absolute path is required (always true in v0.1). */
  requireAbsolute: boolean;
  /** Optional default value to prefill. */
  defaultPath?: string;
  /** Whether a second text field for a label is shown. */
  showLabel: boolean;
  /** Optional default for the label input. */
  defaultLabel?: string;
}

interface PendingPrompt extends PathPromptRequest {
  resolve: (value: PathPromptResult | null) => void;
}

export interface PathPromptResult {
  path: string;
  label: string | null;
}

class PathPromptStore {
  pending = $state<PendingPrompt | null>(null);

  private request(req: PathPromptRequest): Promise<PathPromptResult | null> {
    if (this.pending !== null) {
      return Promise.resolve(null);
    }
    return new Promise((resolve) => {
      this.pending = { ...req, resolve };
    });
  }

  requestWorkspaceOpen(opts: { defaultPath?: string } = {}): Promise<PathPromptResult | null> {
    return this.request({
      kind: 'open_workspace',
      title: 'Open workspace',
      hint: 'Enter the absolute path to the folder you want to open as a workspace. The path must exist on the machine running Agentyx.',
      requireAbsolute: true,
      ...(opts.defaultPath !== undefined ? { defaultPath: opts.defaultPath } : {}),
      showLabel: true,
    });
  }

  requestExtraPath(opts: { defaultPath?: string } = {}): Promise<PathPromptResult | null> {
    return this.request({
      kind: 'add_extra_path',
      title: 'Add extra path',
      hint: 'Enter the absolute path to a directory to grant the agent access to. The path must exist on the machine running Agentyx and cannot be the workspace root.',
      requireAbsolute: true,
      ...(opts.defaultPath !== undefined ? { defaultPath: opts.defaultPath } : {}),
      showLabel: true,
    });
  }

  submit(result: PathPromptResult): void {
    const pending = this.pending;
    if (pending === null) return;
    this.pending = null;
    pending.resolve(result);
  }

  cancel(): void {
    const pending = this.pending;
    if (pending === null) return;
    this.pending = null;
    pending.resolve(null);
  }
}

export const pathPromptStore = new PathPromptStore();
