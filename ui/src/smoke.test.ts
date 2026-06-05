/// Smoke test — placeholder until real component tests land.
/// Vitest exits non-zero when no `*.test.*` files match; this keeps
/// `bun run test` green in v0.1 (F02-only scaffold).
import { describe, it, expect } from 'vitest';

describe('agentyx-ui smoke', () => {
  it('runs', () => {
    expect(true).toBe(true);
  });
});
