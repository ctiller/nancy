# Nancy E2E Testing Strategy

With the complete migration of the `nancy` web architecture from Leptos SSR/Server-Functions to a pure Yew SPA + Axum REST JSON backend, the environment is perfectly primed for strict End-to-End (E2E) UI testing using **Playwright**.

## Architectural Advantages for Testing

1. **Explicit API Boundaries**:
   Instead of intercepting polymorphic WASM encoded server functions, all data ingress and egress is purely JSON-based via explicit paths (`/api/repo/tree`, `/api/tasks`, `/api/grinders`). Playwright `page.route` intercepts can now deterministically mock coordinator API responses, meaning the frontend UI can be thoroughly tested offline without invoking heavy docker containers or Git repository cloning.

2. **Decoupled Builds**:
   Because `trunk build` operates entirely independently of `cargo build` for the coordinator, the E2E layout tests do not require the backend binary to even be compiled to test client bindings. A simple static Node server can host `web/dist` alongside Playwright test executions.

## Execution Structure

```bash
# To run E2E headless validation on the Yew layout:
cd web && trunk build
npx playwright test
```

### Playwright Config (`playwright.config.ts`) (To be Initialized)
```ts
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './tests',
  fullyParallel: true,
  use: {
    baseURL: 'http://127.0.0.1:8080',
    trace: 'on-first-retry',
  },
  webServer: {
    command: 'trunk serve web/',
    url: 'http://127.0.0.1:8080',
    reuseExistingServer: !process.env.CI,
  },
});
```

### Mocking Example

For testing the newly established Monaco Editor + `TaskSubmit` endpoint:
```ts
import { test, expect } from '@playwright/test';

test('Agent submits execution task via Monaco Editor', async ({ page }) => {
  // Mock the Task execution completion
  await page.route('/api/tasks', async route => {
    await route.fulfill({ status: 200, json: { accepted: true } });
  });

  await page.goto('/');

  // Mount monaco and inject text
  await page.click('text="+ New Task"');
  await page.evaluate(() => window.monacoEditor.setValue("Implement robust failover strategies."));
  
  // Submit
  await page.click('text="Submit Task"');
  
  // Verify UI returns to evaluated state gracefully
  await expect(page.locator('text="+ New Task"')).toBeVisible();
});
```

This establishes absolute decoupling. Nancy developers can freely build the frontend without stalling on intensive coordinator lifecycle delays.
