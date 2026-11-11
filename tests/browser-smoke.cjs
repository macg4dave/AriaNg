'use strict';

const assert = require('node:assert/strict');
const path = require('node:path');
const { chromium } = require(path.resolve('.local/browser-tools/node_modules/playwright'));
const origin = 'http://127.0.0.1:8080';

(async () => {
    const browser = await chromium.launch({ channel: 'msedge', headless: true });
    try {
        const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
        await context.route('**/*', route => {
            const url = new URL(route.request().url());
            return url.origin === origin && url.pathname !== '/jsonrpc' ? route.continue() : route.abort();
        });
        await context.addInitScript(() => {
            window.WebSocket = class { constructor() { throw new Error('Fixture blocks RPC'); } };
        });
        const page = await context.newPage();
        let browserErrors = 0;
        let failedAssets = 0;
        page.on('pageerror', () => { browserErrors++; });
        page.on('response', response => { if (response.status() >= 400) { failedAssets++; } });
        await page.goto(origin + '/#!/downloading', { waitUntil: 'networkidle' });
        for (const route of ['downloading', 'waiting', 'stopped', 'new', 'settings/ariang', 'status']) {
            await page.locator('a[href="#!/' + route + '"]').first().click();
            await page.waitForFunction(expected => angular.element(document.body).injector()
                .get('$location').path() === '/' + expected, route);
            await page.waitForTimeout(100);
            assert.equal(await page.evaluate(() => angular.element(document.body).injector()
                .get('$location').path()), '/' + route);
            assert.ok((await page.locator('.content-wrapper').innerText()).trim());
        }
        await page.locator('.sidebar .treeview > a').click();
        for (const route of ['basic', 'http-ftp-sftp', 'http', 'ftp-sftp', 'bt', 'metalink', 'rpc', 'advanced']) {
            await page.locator('a[href="#!/settings/aria2/' + route + '"]').click();
            await page.waitForFunction(expected => angular.element(document.body).injector()
                .get('$location').path() === '/settings/aria2/' + expected, route);
            await page.locator('.settings-table').first().waitFor();
        }
        await page.goBack();
        await page.waitForFunction(() => location.hash === '#!/settings/aria2/rpc');
        await page.goForward();
        await page.waitForFunction(() => location.hash === '#!/settings/aria2/advanced');
        assert.equal(await page.locator('.sweet-overlay:visible').count(), 0);
        await page.goto(origin + '/#!/settings/ariang', { waitUntil: 'networkidle' });
        const theme = page.locator('select[ng-model="context.settings.theme"]');
        await theme.selectOption('dark');
        await page.waitForFunction(() => document.body.classList.contains('theme-dark'));
        await page.reload({ waitUntil: 'networkidle' });
        assert.equal(await theme.inputValue(), 'dark');
        assert.equal(await page.evaluate(() => document.body.classList.contains('theme-dark')), true);
        await theme.selectOption('light');
        await page.waitForFunction(() => !document.body.classList.contains('theme-dark'));
        const language = page.locator('select[ng-model="context.settings.language"]');
        await language.selectOption('string:fr_FR');
        await page.reload({ waitUntil: 'networkidle' });
        assert.equal(await language.inputValue(), 'string:fr_FR');
        assert.ok((await page.locator('.content-wrapper').innerText()).includes('Langage'));
        await language.selectOption('string:en');
        await page.setViewportSize({ width: 390, height: 844 });
        await page.goto(origin + '/#!/downloading', { waitUntil: 'networkidle' });
        assert.equal(await page.locator('.content-wrapper').isVisible(), true);
        assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth), true);
        assert.equal(browserErrors, 0);
        assert.equal(failedAssets, 0);
        await context.close();
        console.log('PASS browser: sidebar/toolbar clicks, all settings routes while disconnected, back/forward, themes, language persistence/translation, mobile width, assets and runtime errors.');
    } finally { await browser.close(); }
})().catch(() => {
    console.error('Browser smoke check failed; browser settings and request details omitted.');
    process.exitCode = 1;
});
