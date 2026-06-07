import { describe, expect, it } from 'vitest';

import type { GlobalConfigDto, PermissionMatrixDto } from '$lib/ipc-types';

import {
  emptyProviderPatch,
  parseIgnorePatterns,
  providerHasSecret,
  providerLabel,
  requiresDevChannelConfirmation,
  sortedProviderIds,
  sortedToolIds,
  staticDefaultDecision,
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

  it('f05_ac9_permission_matrix_edits_persist helper returns stable order', () => {
    // The matrix table renders rows in this order; verify the
    // helper is deterministic and alphabetical.
    const matrix: PermissionMatrixDto = {
      global: {
        shell: 'deny',
        read_file: 'allow',
        write_file: 'ask',
        list_dir: 'allow',
        search: 'allow',
        edit_file: 'ask',
        apply_patch: 'ask',
        python_run: 'ask',
      },
      effective: {
        shell: 'deny',
        read_file: 'allow',
        write_file: 'ask',
        list_dir: 'allow',
        search: 'allow',
        edit_file: 'ask',
        apply_patch: 'ask',
        python_run: 'ask',
      },
    };
    expect(sortedToolIds(matrix)).toEqual([
      'apply_patch',
      'edit_file',
      'list_dir',
      'python_run',
      'read_file',
      'search',
      'shell',
      'write_file',
    ]);
    expect(sortedToolIds(null)).toEqual([]);
  });

  it('f05_ac9_static_default_decision_matches_known_tools', () => {
    // Read-only tools default to allow; write tools default to ask.
    expect(staticDefaultDecision('read_file')).toBe('allow');
    expect(staticDefaultDecision('list_dir')).toBe('allow');
    expect(staticDefaultDecision('search')).toBe('allow');
    expect(staticDefaultDecision('write_file')).toBe('ask');
    expect(staticDefaultDecision('edit_file')).toBe('ask');
    expect(staticDefaultDecision('shell')).toBe('ask');
    expect(staticDefaultDecision('python_run')).toBe('ask');
    expect(staticDefaultDecision('apply_patch')).toBe('ask');
    // Unknown tools fall back to ask (defensive default).
    expect(staticDefaultDecision('not_in_catalog')).toBe('ask');
  });
});
