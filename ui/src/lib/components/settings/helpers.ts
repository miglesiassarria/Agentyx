import type {
  GlobalConfigDto,
  PermissionMatrixDto,
  ProviderConfigDto,
  ProviderId,
  TestConnectionResult,
  ToolId,
} from '$lib/ipc-types';

export const PROVIDER_DEFAULTS: Record<ProviderId, ProviderConfigDto> = {
  ollama: {
    baseUrl: 'http://127.0.0.1:11434',
    enabled: true,
  },
  groq: {
    baseUrl: 'https://api.groq.com/openai/v1',
    enabled: true,
  },
  minimax: {
    baseUrl: 'https://api.minimax.io/anthropic',
    enabled: true,
  },
};

export const PROVIDER_LABELS: Record<ProviderId, string> = {
  ollama: 'Ollama',
  groq: 'Groq',
  minimax: 'Minimax',
};

export const PROVIDER_MODEL_DEFAULTS: Record<ProviderId, string[]> = {
  ollama: ['llama3.1:8b'],
  groq: ['llama-3.3-70b-versatile', 'llama-3.1-8b-instant', 'mixtral-8x7b-32768'],
  minimax: [
    'MiniMax-M3',
    'MiniMax-M2.7',
    'MiniMax-M2.7-highspeed',
    'MiniMax-M2.5',
    'MiniMax-M2.5-highspeed',
    'MiniMax-M2.1',
    'MiniMax-M2.1-highspeed',
    'MiniMax-M2',
  ],
};

export function providerLabel(providerId: ProviderId): string {
  return PROVIDER_LABELS[providerId] ?? providerId;
}

export function sortedProviderIds(config: GlobalConfigDto | null): ProviderId[] {
  if (config === null) return [];
  return Object.keys(config.providers).sort((a, b) => {
    if (a === config.defaultProvider) return -1;
    if (b === config.defaultProvider) return 1;
    return providerLabel(a).localeCompare(providerLabel(b));
  });
}

export function providerHasSecret(
  providerId: ProviderId,
  provider: ProviderConfigDto,
  keychainProviderIds: ProviderId[],
): boolean {
  if (keychainProviderIds.includes(providerId)) return true;
  return provider.apiKey !== undefined;
}

export function emptyProviderPatch(
  config: GlobalConfigDto,
  providerId: ProviderId,
  provider: ProviderConfigDto,
): Record<ProviderId, ProviderConfigDto> {
  return {
    [providerId]: {
      ...config.providers[providerId],
      ...provider,
    },
  };
}

export function availableModels(
  providerId: ProviderId,
  provider: ProviderConfigDto | undefined,
  testResult: TestConnectionResult | undefined,
  selectedModel?: string,
): string[] {
  const ordered = [
    ...(testResult?.ok === true ? testResult.models : []),
    ...(provider?.models ?? []),
    ...(PROVIDER_MODEL_DEFAULTS[providerId] ?? []),
    ...(selectedModel === undefined || selectedModel.trim() === '' ? [] : [selectedModel.trim()]),
  ];
  return [...new Set(ordered.map((model) => model.trim()).filter((model) => model.length > 0))];
}

export function parseIgnorePatterns(value: string): string[] {
  return value
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

export function requiresDevChannelConfirmation(channel: string): boolean {
  return channel === 'dev';
}

export function formatError(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/**
 * Stable, alphabetical list of tool ids from a permission
 * matrix. Used by the `ApprovalTab` table in `SettingsView` to
 * render rows in a deterministic order across renders and users
 * (F05.AC9).
 */
export function sortedToolIds(matrix: PermissionMatrixDto | null): ToolId[] {
  if (matrix === null) return [];
  return Object.keys(matrix.effective).sort();
}

/**
 * Static v0.1 default per-tool decision. Kept in sync with
 * `crates/agentyx-app/src/commands/permissions.rs::default_decision_for`.
 * Used by the matrix UI to display the fallback value when the
 * user has not persisted an override.
 */
export function staticDefaultDecision(tool: ToolId): 'allow' | 'ask' | 'deny' {
  switch (tool) {
    case 'read_file':
    case 'list_dir':
    case 'search':
      return 'allow';
    case 'write_file':
    case 'edit_file':
    case 'shell':
    case 'python_run':
    case 'apply_patch':
      return 'ask';
    default:
      return 'ask';
  }
}
