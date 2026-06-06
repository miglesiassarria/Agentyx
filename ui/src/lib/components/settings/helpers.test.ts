import { describe, expect, it } from 'vitest';

import type { GlobalConfigDto } from '$lib/ipc-types';

import {
  emptyProviderPatch,
  parseIgnorePatterns,
  providerHasSecret,
  providerLabel,
  requiresDevChannelConfirmation,
  sortedProviderIds,
} from './helpers';

const baseConfig: GlobalConfigDto = {
  version: 1,
  approvalMode: 'ask',
  defaultProvider: 'ollama',
  defaultModel: 'llama3.1:8b',
  providers: {
    groq: { baseUrl: 'https://api.groq.com/openai/v1', enabled: true },
    ollama: { baseUrl: 'http://127.0.0.1:11434', enabled: true },
  },
  ui: {
    theme: 'auto',
    fontSize: 14,
    showTokenCount: true,
    showTimestamps: true,
  },
  telemetryEnabled: false,
  checkUpdates: true,
  updateChannel: 'stable',
};

describe('settings helpers', () => {
  it('f05_ac1_settings_shows_ollama_preconfigured helper order', () => {
    expect(sortedProviderIds(baseConfig)[0]).toBe('ollama');
    expect(providerLabel('ollama')).toBe('Ollama');
  });

  it('f05_ac12_ui_never_displays_secret_value helper only tracks presence', () => {
    const groq = baseConfig.providers.groq;
    const ollama = baseConfig.providers.ollama;
    if (groq === undefined || ollama === undefined) throw new Error('missing fixture provider');
    expect(providerHasSecret('groq', groq, ['groq'])).toBe(true);
    expect(providerHasSecret('ollama', ollama, [])).toBe(false);
  });

  it('merges provider patch without copying unrelated providers', () => {
    expect(
      emptyProviderPatch(baseConfig, 'groq', {
        baseUrl: 'https://example.invalid',
        enabled: false,
      }),
    ).toEqual({
      groq: {
        baseUrl: 'https://example.invalid',
        enabled: false,
      },
    });
  });

  it('parses ignore patterns from textarea content', () => {
    expect(parseIgnorePatterns('target/\n\n node_modules/ \n')).toEqual([
      'target/',
      'node_modules/',
    ]);
  });

  it('f05_ac14_update_channel_dev_requires_confirmation helper', () => {
    expect(requiresDevChannelConfirmation('dev')).toBe(true);
    expect(requiresDevChannelConfirmation('stable')).toBe(false);
  });
});
