<script lang="ts">
  import { onDestroy, onMount } from 'svelte';

  import {
    config as configIpc,
    events,
    permissions as permissionsIpc,
    providers,
    secrets,
  } from '$lib/ipc';
  import type {
    ApprovalMode,
    ConfigChangedPayload,
    GlobalConfigDto,
    PermissionMatrixDto,
    ProviderConfigDto,
    ProviderId,
    ResolvedConfigDto,
    TestConnectionResult,
    ToolDecision,
    UpdateChannel,
    WorkspaceDto,
  } from '$lib/ipc-types';

  import ExtrasSection from '../ExtrasSection.svelte';
  import {
    PROVIDER_DEFAULTS,
    availableModels,
    emptyProviderPatch,
    formatError,
    parseIgnorePatterns,
    providerHasSecret,
    providerLabel,
    requiresDevChannelConfirmation,
    sortedProviderIds,
    sortedToolIds,
  } from './helpers';

  interface Props {
    workspace: WorkspaceDto | null;
  }

  type Tab = 'providers' | 'models' | 'approval' | 'workspace';

  let { workspace }: Props = $props();

  let activeTab = $state<Tab>('providers');
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let savedMessage = $state<string | null>(null);

  let globalConfig = $state<GlobalConfigDto | null>(null);
  let resolvedConfig = $state<ResolvedConfigDto | null>(null);
  let permissionMatrix = $state<PermissionMatrixDto | null>(null);
  let testResults = $state<Record<ProviderId, TestConnectionResult>>({});
  let testing = $state<Record<ProviderId, boolean>>({});
  let providerDrafts = $state<Record<ProviderId, ProviderConfigDto>>({});

  let addKind = $state<ProviderId>('groq');
  let addBaseUrl = $state(PROVIDER_DEFAULTS.groq.baseUrl);
  let addApiKey = $state('');
  let addFailed = $state(false);

  let selectedDefaultProvider = $state<ProviderId>('ollama');
  let selectedDefaultModel = $state('');
  let selectedApprovalMode = $state<ApprovalMode>('ask');
  let selectedUpdateChannel = $state<UpdateChannel>('stable');
  let confirmDevChannel = $state(false);

  let workspaceDefaultModel = $state('');
  let workspaceApprovalMode = $state<ApprovalMode | 'inherit'>('inherit');
  let ignorePatternsText = $state('');
  let journalMaxRows = $state('100000');

  // F05.AC9 — per-row state for the editable permission matrix.
  // `matrixSaving[tool]` is `true` while a `set_default` call is
  // in flight for that tool; used to disable the radios and show
  // a per-row spinner.
  let matrixSaving = $state<Record<string, boolean>>({});

  let unlistenConfigChanged: (() => void) | null = null;

  let defaultProviderModels = $derived(
    availableModels(
      selectedDefaultProvider,
      globalConfig?.providers[selectedDefaultProvider],
      testResults[selectedDefaultProvider],
      selectedDefaultProvider === globalConfig?.defaultProvider ? selectedDefaultModel : undefined,
    ),
  );
  let workspaceModelProvider = $derived(
    resolvedConfig?.workspace?.defaultProvider ??
      resolvedConfig?.effective.defaultProvider ??
      selectedDefaultProvider,
  );
  let workspaceProviderModels = $derived(
    availableModels(
      workspaceModelProvider,
      globalConfig?.providers[workspaceModelProvider],
      testResults[workspaceModelProvider],
      workspaceDefaultModel || resolvedConfig?.effective.defaultModel,
    ),
  );

  $effect(() => {
    if (globalConfig !== null) {
      selectedDefaultProvider = globalConfig.defaultProvider;
      selectedDefaultModel = globalConfig.defaultModel;
      selectedApprovalMode = globalConfig.approvalMode;
      selectedUpdateChannel = globalConfig.updateChannel;
    }
  });

  $effect(() => {
    addBaseUrl = PROVIDER_DEFAULTS[addKind]?.baseUrl ?? '';
    addFailed = false;
  });

  $effect(() => {
    if (defaultProviderModels.length === 0) return;
    if (!defaultProviderModels.includes(selectedDefaultModel)) {
      selectedDefaultModel = defaultProviderModels[0] ?? selectedDefaultModel;
    }
  });

  $effect(() => {
    const workspaceConfig = resolvedConfig?.workspace;
    const effective = resolvedConfig?.effective;
    if (workspaceConfig !== undefined && effective !== undefined) {
      workspaceDefaultModel = workspaceConfig?.defaultModel ?? '';
      workspaceApprovalMode = workspaceConfig?.approvalMode ?? 'inherit';
      ignorePatternsText = effective.workspaceSettings.ignorePatterns.join('\n');
      journalMaxRows = String(effective.workspaceSettings.journalMaxRows ?? 100000);
    }
  });

  onMount(() => {
    void loadSettings();
    // F05.AC15 — subscribe to `config.changed.v1` so an external
    // writer (other tab, CLI, etc.) propagates into the panel.
    void events
      .configChanged((payload) => {
        void onConfigChanged(payload);
      })
      .then((unlisten) => {
        unlistenConfigChanged = unlisten;
      });
  });

  onDestroy(() => {
    unlistenConfigChanged?.();
    unlistenConfigChanged = null;
  });

  async function onConfigChanged(payload: ConfigChangedPayload): Promise<void> {
    if (payload.kind === 'global' && payload.global !== undefined) {
      globalConfig = payload.global;
      providerDrafts = cloneProviders(payload.global.providers);
    }
    // Refresh the matrix: the persisted default may have changed.
    try {
      permissionMatrix =
        workspace !== null
          ? await permissionsIpc.getMatrix(workspace.id)
          : await permissionsIpc.getMatrix();
    } catch (e) {
      error = formatError(e);
    }
  }

  async function loadSettings(): Promise<void> {
    loading = true;
    error = null;
    try {
      globalConfig = await configIpc.getGlobal();
      providerDrafts = cloneProviders(globalConfig.providers);
      if (workspace !== null) {
        resolvedConfig = await configIpc.getWorkspace(workspace.id);
        permissionMatrix = await permissionsIpc.getMatrix(workspace.id);
      } else {
        resolvedConfig = null;
        permissionMatrix = await permissionsIpc.getMatrix();
      }
    } catch (e) {
      error = formatError(e);
    } finally {
      loading = false;
    }
  }

  async function saveGlobalBasics(): Promise<void> {
    if (globalConfig === null) return;
    if (requiresDevChannelConfirmation(selectedUpdateChannel) && !confirmDevChannel) {
      error = 'Confirm the dev update channel before saving.';
      return;
    }
    await saveGlobal({
      defaultProvider: selectedDefaultProvider,
      defaultModel: selectedDefaultModel,
      approvalMode: selectedApprovalMode,
      updateChannel: selectedUpdateChannel,
    });
  }

  async function saveGlobal(patch: Parameters<typeof configIpc.updateGlobal>[0]): Promise<void> {
    saving = true;
    error = null;
    savedMessage = null;
    try {
      globalConfig = await configIpc.updateGlobal(patch);
      savedMessage = 'Saved';
      await loadSettings();
    } catch (e) {
      error = formatError(e);
    } finally {
      saving = false;
    }
  }

  async function saveWorkspaceSettings(): Promise<void> {
    if (workspace === null) return;
    const rows = Number(journalMaxRows);
    if (!Number.isInteger(rows) || rows < 1000) {
      error = 'journal_max_rows must be at least 1000.';
      return;
    }
    saving = true;
    error = null;
    savedMessage = null;
    try {
      await configIpc.updateWorkspace(workspace.id, {
        defaultModel: workspaceDefaultModel.trim() === '' ? null : workspaceDefaultModel.trim(),
        approvalMode: workspaceApprovalMode === 'inherit' ? null : workspaceApprovalMode,
        workspace: {
          ignorePatterns: parseIgnorePatterns(ignorePatternsText),
          journalMaxRows: rows,
        },
      });
      savedMessage = 'Saved';
      await loadSettings();
    } catch (e) {
      error = formatError(e);
    } finally {
      saving = false;
    }
  }

  async function updateProvider(
    providerId: ProviderId,
    provider: ProviderConfigDto,
  ): Promise<void> {
    if (globalConfig === null) return;
    await saveGlobal({ providers: emptyProviderPatch(globalConfig, providerId, provider) });
  }

  function updateProviderDraft(providerId: ProviderId, provider: ProviderConfigDto): void {
    providerDrafts = {
      ...providerDrafts,
      [providerId]: provider,
    };
  }

  function cloneProviders(
    input: Record<ProviderId, ProviderConfigDto>,
  ): Record<ProviderId, ProviderConfigDto> {
    return Object.fromEntries(
      Object.entries(input).map(([id, provider]) => [
        id,
        {
          ...provider,
          models: provider.models === undefined ? undefined : [...provider.models],
        },
      ]),
    );
  }

  async function testProvider(providerId: ProviderId, provider: ProviderConfigDto): Promise<void> {
    testing = { ...testing, [providerId]: true };
    error = null;
    try {
      testResults = {
        ...testResults,
        [providerId]: await providers.testConnection({ providerId, provider }),
      };
    } catch (e) {
      error = formatError(e);
    } finally {
      testing = { ...testing, [providerId]: false };
    }
  }

  async function testAndAddProvider(addAnyway = false): Promise<void> {
    if (globalConfig === null) return;
    const provider: ProviderConfigDto = {
      baseUrl: addBaseUrl,
      enabled: true,
      apiKey: addKind === 'ollama' ? undefined : { keychain: { account: addKind } },
    };
    if (!addAnyway) {
      const result = await providers.testConnection({
        providerId: addKind,
        provider,
        inlineApiKey: addApiKey.trim() === '' ? undefined : addApiKey,
      });
      testResults = { ...testResults, [addKind]: result };
      if (!result.ok) {
        addFailed = true;
        return;
      }
    }
    if (addApiKey.trim() !== '') {
      await secrets.set(addKind, addApiKey);
      addApiKey = '';
    }
    await saveGlobal({ providers: emptyProviderPatch(globalConfig, addKind, provider) });
    addFailed = false;
  }

  // F05.AC9 — set a tool's default decision. The Tauri command
  // persists to `GlobalConfig.default_tool_decisions`. After it
  // returns, we refresh the matrix to pick up the new value (we
  // also rely on `config.changed.v1` for cross-tab sync, but the
  // direct refresh avoids a round-trip wait).
  async function setToolDecision(tool: string, decision: ToolDecision): Promise<void> {
    matrixSaving = { ...matrixSaving, [tool]: true };
    error = null;
    try {
      await permissionsIpc.setDefault(tool, decision);
      // Re-fetch the matrix so the UI shows the new value
      // (also covers the case where the listener is racing).
      permissionMatrix =
        workspace !== null
          ? await permissionsIpc.getMatrix(workspace.id)
          : await permissionsIpc.getMatrix();
    } catch (e) {
      error = formatError(e);
    } finally {
      matrixSaving = { ...matrixSaving, [tool]: false };
    }
  }
</script>

<main class="settings">
  <header class="hero">
    <div>
      <p class="eyebrow">Configuration</p>
      <h1>Settings</h1>
      <p class="subtitle">Providers, models, approvals and workspace defaults.</p>
    </div>
    <button type="button" class="ghost" onclick={loadSettings} disabled={loading || saving}
      >Refresh</button
    >
  </header>

  {#if error !== null}
    <div class="banner error" role="alert">{error}</div>
  {/if}
  {#if savedMessage !== null}
    <div class="banner success" role="status">{savedMessage}</div>
  {/if}

  <nav class="tabs" aria-label="Settings tabs">
    {#each ['providers', 'models', 'approval', 'workspace'] as tab}
      <button
        type="button"
        class:active={activeTab === tab}
        onclick={() => (activeTab = tab as Tab)}
      >
        {tab}
      </button>
    {/each}
  </nav>

  {#if loading || globalConfig === null}
    <section class="panel"><p class="muted">Loading settings…</p></section>
  {:else if activeTab === 'providers'}
    <section class="panel stack">
      <div class="section-head">
        <h2>Providers</h2>
        <p>API keys are stored in keychain and never displayed.</p>
      </div>

      {#each sortedProviderIds(globalConfig) as providerId}
        {@const persistedProvider = globalConfig.providers[providerId]}
        {@const provider = providerDrafts[providerId] ?? persistedProvider}
        <article class="card provider-card">
          <div class="card-title">
            <div>
              <h3>{providerLabel(providerId)}</h3>
              <input
                aria-label="{providerId} base URL"
                value={provider.baseUrl}
                oninput={(e) =>
                  updateProviderDraft(providerId, {
                    ...provider,
                    baseUrl: e.currentTarget.value,
                  })}
              />
            </div>
            <label class="switch">
              <input
                type="checkbox"
                checked={provider.enabled}
                onchange={(e) =>
                  updateProviderDraft(providerId, {
                    ...provider,
                    enabled: e.currentTarget.checked,
                  })}
              />
              enabled
            </label>
          </div>

          <div class="meta-row">
            <span>Default: {globalConfig.defaultProvider === providerId ? 'yes' : 'no'}</span>
            <span>
              API key:
              {providerHasSecret(providerId, provider, resolvedConfig?.keychainProviderIds ?? [])
                ? 'set in keychain'
                : 'not set'}
            </span>
          </div>

          {#if testResults[providerId]}
            <div class:ok={testResults[providerId].ok} class="test-result">
              {#if testResults[providerId].ok}
                Connected ({testResults[providerId].latencyMs ?? 0}ms). Models:
                {testResults[providerId].models.slice(0, 4).join(', ') || 'none reported'}
              {:else}
                Failed: {testResults[providerId].error ?? testResults[providerId].errorCode}
              {/if}
            </div>
          {/if}

          <div class="actions">
            <button
              type="button"
              class="primary"
              onclick={() => testProvider(providerId, provider)}
              disabled={testing[providerId]}
            >
              {testing[providerId] ? 'Testing…' : 'Test connection'}
            </button>
            <button
              type="button"
              class="ghost"
              onclick={() => updateProvider(providerId, provider)}
            >
              Save provider
            </button>
            {#if providerId !== 'ollama'}
              <input
                type="password"
                aria-label="Set {providerId} API key"
                placeholder="Paste new API key"
                onkeydown={async (e) => {
                  if (e.key !== 'Enter') return;
                  const input = e.currentTarget;
                  const value = input.value.trim();
                  if (value === '') return;
                  await secrets.set(providerId, value);
                  input.value = '';
                  await updateProvider(providerId, {
                    ...provider,
                    apiKey: { keychain: { account: providerId } },
                  });
                }}
              />
            {/if}
          </div>
        </article>
      {/each}

      <article class="card add-card">
        <h3>Add provider</h3>
        <div class="grid">
          <label>
            Kind
            <select bind:value={addKind}>
              <option value="groq">Groq</option>
              <option value="minimax">Minimax</option>
              <option value="ollama">Ollama</option>
            </select>
          </label>
          <label>
            Base URL
            <input bind:value={addBaseUrl} />
          </label>
          <label>
            API key
            <input bind:value={addApiKey} type="password" placeholder="Optional for Ollama" />
          </label>
        </div>
        {#if addFailed && testResults[addKind]}
          <p class="warning">
            Test failed: {testResults[addKind].error}. You can retry or add anyway.
          </p>
        {/if}
        <div class="actions">
          <button type="button" class="primary" onclick={() => testAndAddProvider(false)}>
            Test & Add
          </button>
          {#if addFailed}
            <button type="button" class="ghost" onclick={() => testAndAddProvider(true)}>
              Add anyway
            </button>
          {/if}
        </div>
      </article>
    </section>
  {:else if activeTab === 'models'}
    <section class="panel stack">
      <div class="section-head">
        <h2>Models</h2>
        <p>Global defaults apply unless the workspace overrides them.</p>
      </div>
      <article class="card grid">
        <label>
          Default provider
          <select bind:value={selectedDefaultProvider}>
            {#each sortedProviderIds(globalConfig) as providerId}
              <option value={providerId}>{providerLabel(providerId)}</option>
            {/each}
          </select>
        </label>
        <label>
          Default model
          <select bind:value={selectedDefaultModel} disabled={defaultProviderModels.length === 0}>
            {#each defaultProviderModels as model}
              <option value={model}>{model}</option>
            {/each}
          </select>
        </label>
        <button
          type="button"
          class="primary align-end"
          onclick={saveGlobalBasics}
          disabled={saving}
        >
          Save global defaults
        </button>
      </article>
      <article class="card grid">
        <label>
          Workspace model override
          <select bind:value={workspaceDefaultModel} disabled={workspace === null}>
            <option value="">Inherit global model</option>
            {#each workspaceProviderModels as model}
              <option value={model}>{model}</option>
            {/each}
          </select>
        </label>
        <button
          type="button"
          class="primary align-end"
          onclick={saveWorkspaceSettings}
          disabled={workspace === null || saving}
        >
          Save workspace override
        </button>
      </article>
    </section>
  {:else if activeTab === 'approval'}
    <section class="panel stack">
      <div class="section-head">
        <h2>Approval</h2>
        <p>Changes apply to new runs. Current runs keep their snapshot.</p>
      </div>
      <article class="card grid">
        <label>
          Global approval mode
          <select bind:value={selectedApprovalMode}>
            <option value="ask">Ask</option>
            <option value="allow">Allow</option>
            <option value="deny">Deny</option>
          </select>
        </label>
        <label>
          Workspace override
          <select bind:value={workspaceApprovalMode} disabled={workspace === null}>
            <option value="inherit">Inherit global</option>
            <option value="ask">Ask</option>
            <option value="allow">Allow</option>
            <option value="deny">Deny</option>
          </select>
        </label>
        <button
          type="button"
          class="primary align-end"
          onclick={saveGlobalBasics}
          disabled={saving}
        >
          Save global
        </button>
        <button
          type="button"
          class="ghost align-end"
          onclick={saveWorkspaceSettings}
          disabled={workspace === null || saving}
        >
          Save workspace
        </button>
      </article>
      <article class="card">
        <h3>Tool matrix</h3>
        <p class="muted">
          Each tool has a default decision (allow / ask / deny). Changes persist to <code
            >GlobalConfig</code
          > and apply to new runs.
        </p>
        <table>
          <thead>
            <tr><th>Tool</th><th>Default</th><th>Workspace override</th></tr>
          </thead>
          <tbody>
            {#each sortedToolIds(permissionMatrix) as tool}
              {@const global = permissionMatrix?.global[tool] ?? 'ask'}
              {@const workspace = permissionMatrix?.workspace?.[tool]}
              {@const effective = permissionMatrix?.effective[tool] ?? 'ask'}
              <tr>
                <td><code>{tool}</code></td>
                <td>
                  <div class="radio-row" role="radiogroup" aria-label="Default decision for {tool}">
                    {#each ['allow', 'ask', 'deny'] as decision (decision)}
                      <label class="radio">
                        <input
                          type="radio"
                          name="tool-{tool}"
                          value={decision}
                          checked={global === decision}
                          disabled={matrixSaving[tool] === true}
                          onchange={() => void setToolDecision(tool, decision as ToolDecision)}
                        />
                        {decision}
                      </label>
                    {/each}
                  </div>
                </td>
                <td>
                  {#if workspace !== undefined}
                    {workspace}
                  {:else}
                    <span class="muted">—</span>
                  {/if}
                  {#if effective !== global}
                    <span class="effective-note">→ {effective}</span>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      </article>
    </section>
  {:else}
    <section class="panel stack">
      <div class="section-head">
        <h2>Workspace</h2>
        <p>
          {workspace === null
            ? 'Select a workspace to edit workspace settings.'
            : workspace.rootPath}
        </p>
      </div>
      <article class="card grid">
        <label class="wide">
          Ignore patterns
          <textarea bind:value={ignorePatternsText} disabled={workspace === null}></textarea>
        </label>
        <label>
          Journal max rows
          <input bind:value={journalMaxRows} disabled={workspace === null} inputmode="numeric" />
        </label>
        <label>
          Update channel
          <select bind:value={selectedUpdateChannel}>
            <option value="stable">Stable</option>
            <option value="beta">Beta</option>
            <option value="dev">Dev</option>
          </select>
        </label>
        {#if requiresDevChannelConfirmation(selectedUpdateChannel)}
          <label class="warning-check wide">
            <input type="checkbox" bind:checked={confirmDevChannel} />
            You're switching to a less stable channel.
          </label>
        {/if}
        <button
          type="button"
          class="primary align-end"
          onclick={saveWorkspaceSettings}
          disabled={workspace === null || saving}
        >
          Save workspace
        </button>
        <button type="button" class="ghost align-end" onclick={saveGlobalBasics} disabled={saving}>
          Save app settings
        </button>
      </article>
      {#if workspace !== null}
        <article class="card">
          <h3>Extra paths</h3>
          <ExtrasSection {workspace} />
        </article>
      {/if}
    </section>
  {/if}
</main>

<style>
  .settings {
    height: 100%;
    overflow: auto;
    padding: var(--space-5);
    background:
      radial-gradient(circle at top right, rgba(47, 129, 247, 0.12), transparent 32rem),
      var(--color-bg);
  }

  .hero {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-4);
    margin-bottom: var(--space-4);
  }

  .eyebrow,
  .subtitle,
  .muted,
  .section-head p {
    margin: 0;
    color: var(--color-fg-subtle);
    font-size: var(--font-size-sm);
  }

  h1,
  h2,
  h3 {
    margin: 0;
  }

  h1 {
    font-size: var(--font-size-2xl);
  }

  .tabs {
    display: flex;
    gap: var(--space-2);
    margin: var(--space-4) 0;
  }

  .tabs button,
  .ghost,
  .primary {
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-full);
    padding: var(--space-2) var(--space-4);
    background: var(--color-bg-elevated);
    color: var(--color-fg);
  }

  .tabs button.active,
  .primary {
    background: var(--color-primary);
    border-color: var(--color-primary);
    color: var(--color-primary-fg);
  }

  .panel,
  .card {
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--color-bg-elevated) 92%, transparent);
  }

  .panel {
    padding: var(--space-4);
  }

  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .section-head {
    display: flex;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .card {
    padding: var(--space-4);
  }

  .provider-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .card-title,
  .actions,
  .meta-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }

  .meta-row {
    justify-content: flex-start;
    color: var(--color-fg-subtle);
    font-size: var(--font-size-sm);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: var(--space-4);
  }

  label {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    color: var(--color-fg-muted);
    font-size: var(--font-size-sm);
  }

  input,
  select,
  textarea {
    width: 100%;
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-md);
    padding: var(--space-2) var(--space-3);
    background: var(--color-bg);
    color: var(--color-fg);
    font: inherit;
  }

  textarea {
    min-height: 180px;
    font-family: var(--font-mono);
  }

  .wide {
    grid-column: 1 / -1;
  }

  .align-end {
    align-self: end;
  }

  .switch {
    flex-direction: row;
    align-items: center;
  }

  .switch input,
  .warning-check input {
    width: auto;
  }

  .banner {
    margin: var(--space-3) 0;
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-subtle);
  }

  .error,
  .warning {
    color: var(--color-danger);
  }

  .success,
  .ok {
    color: var(--color-success);
  }

  .test-result {
    color: var(--color-danger);
    font-size: var(--font-size-sm);
  }

  table {
    width: 100%;
    border-collapse: collapse;
    margin-top: var(--space-3);
    font-size: var(--font-size-sm);
  }

  th,
  td {
    padding: var(--space-2);
    border-bottom: 1px solid var(--color-border-subtle);
    text-align: left;
  }

  .radio-row {
    display: inline-flex;
    gap: var(--space-3);
  }

  .radio {
    display: inline-flex;
    flex-direction: row;
    align-items: center;
    gap: var(--space-1);
    color: var(--color-fg);
    font-size: var(--font-size-sm);
  }

  .radio input {
    width: auto;
  }

  .effective-note {
    margin-left: var(--space-2);
    color: var(--color-fg-subtle);
    font-size: var(--font-size-sm);
  }

  @media (max-width: 760px) {
    .settings {
      padding: var(--space-3);
      background: var(--color-bg);
    }

    .hero,
    .section-head,
    .card-title,
    .actions,
    .meta-row {
      align-items: stretch;
      flex-direction: column;
    }

    .hero {
      gap: var(--space-3);
    }

    h1 {
      font-size: var(--font-size-xl);
    }

    .tabs {
      overflow-x: auto;
      padding-bottom: var(--space-1);
      scrollbar-width: thin;
    }

    .tabs button {
      flex: 0 0 auto;
    }

    .panel,
    .card {
      border-radius: var(--radius-md);
    }

    .panel,
    .card {
      padding: var(--space-3);
    }

    .grid {
      grid-template-columns: 1fr;
      gap: var(--space-3);
    }

    input,
    select,
    textarea {
      font-size: 16px;
    }

    .align-end {
      align-self: stretch;
    }

    .primary,
    .ghost {
      width: 100%;
    }

    table {
      display: block;
      overflow-x: auto;
      white-space: nowrap;
    }

    .radio-row {
      gap: var(--space-2);
    }
  }
</style>
