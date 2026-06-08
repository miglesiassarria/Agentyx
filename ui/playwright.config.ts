import { defineConfig, devices } from '@playwright/test';
import os from 'os';
import path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const PORT = process.env.AGENTYX_SMOKE_PORT || '18765';
const BASE_URL = `http://127.0.0.1:${PORT}`;

const AGENTYX_HOME =
  process.env.AGENTYX_E2E_HOME ?? path.join(os.tmpdir(), `agentyx-e2e-${PORT}-${Date.now()}`);
const CRATES_DIR = path.resolve(__dirname, '../crates');
const REAL_HOME = os.homedir();

export default defineConfig({
  testDir: './e2e',
  timeout: 60_000,
  fullyParallel: false,
  retries: 0,
  workers: 1,
  reporter: [['list']],
  use: {
    baseURL: BASE_URL,
    trace: 'on-first-retry',
    launchOptions: {
      args: ['--headless'],
    },
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
  ],
  webServer: {
    command: `cargo run --bin agentyx-web -- --host 127.0.0.1 --port ${PORT}`,
    cwd: CRATES_DIR,
    env: {
      AGENTYX_HOME,
      CARGO_HOME: process.env.CARGO_HOME ?? path.join(REAL_HOME, '.cargo'),
      HOME: AGENTYX_HOME,
      RUSTUP_HOME: process.env.RUSTUP_HOME ?? path.join(REAL_HOME, '.rustup'),
      RUST_LOG: 'info',
      USERPROFILE: AGENTYX_HOME,
    },
    url: BASE_URL,
    reuseExistingServer: false,
    timeout: 60_000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
});
