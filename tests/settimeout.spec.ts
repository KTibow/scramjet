import { test, expect } from '@playwright/test';
import { setupPage } from './util/setupPage'; // Assuming this utility helps with proxy setup

test.describe('setTimeout behavior through Scramjet proxy', () => {
  test('should execute setTimeout calls without context errors', async ({ page, baseURL }) => {
    if (!baseURL) {
      throw new Error('baseURL is not defined. Ensure Playwright is configured with a baseURL for the proxy.');
    }

    // The 'setupPage' utility might be essential for configuring the browser context
    // to use the proxy, especially if the proxy requires specific headers or settings.
    // If it's a transparent proxy set at the browser launch level by Playwright config,
    // this might not be strictly needed for navigation.
    // However, many test suites use such a utility for consistency or complex setups.
    // For now, we assume it's either implicitly handled by Playwright's baseURL
    // or needs to be called if requests aren't going through the proxy.
    // await setupPage(page); // Keeping it commented unless tests fail to proxy

    const targetUrl = '/static/settimeout_test_page.html';

    // Navigate to the page through the proxy.
    // Playwright's page.goto() will combine baseURL and targetUrl.
    // e.g., if baseURL is http://localhost:8080, it will navigate to http://localhost:8080/static/settimeout_test_page.html
    await page.goto(targetUrl);

    // Wait for the signal that tests are done on the page
    await page.waitForSelector('#testsComplete', { timeout: 5000 });

    // Capture console messages for error checking, particularly if page script fails to catch something.
    const consoleMessages = [];
    page.on('console', msg => {
      if (msg.type() === 'error') {
        consoleMessages.push(msg.text());
      }
    });

    const results = await page.evaluate(() => window.setTimeoutTestResults);

    expect(results.errors, `Page-collected errors: ${results.errors.join(', ')}`).toEqual([]);
    // Check console errors collected by Playwright, as a secondary check.
    // This might be redundant if the page script's error listeners are comprehensive.
    expect(consoleMessages, `Playwright-collected console errors: ${consoleMessages.join(', ')}`).toEqual([]);

    expect(results.direct, 'Direct setTimeout did not execute').toBe(true);
    expect(results.windowScoped, 'window.setTimeout did not execute').toBe(true);
    expect(results.thisScopedGlobal, 'this.setTimeout (global context) did not execute').toBe(true);
  });
});
