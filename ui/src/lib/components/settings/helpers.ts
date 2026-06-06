import type { GlobalConfigDto, ProviderConfigDto, ProviderId } from '$lib/ipc-types';

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
    baseUrl: 'https://api.minimax.io/v1',
    enabled: true,
  },
};

export const PROVIDER_LABELS: Record<ProviderId, string> = {
  ollama: 'Ollama',
  groq: 'Groq',
  minimax: 'Minimax',
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
