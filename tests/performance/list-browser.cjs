'use strict';

const assert = require('node:assert/strict');
const path = require('node:path');
// Optional browser tooling lives outside the application dependency graph.
const { chromium } = require(path.resolve('.local/browser-tools/node_modules/playwright'));

(async () => {
    const browser = await chromium.launch({ channel: 'msedge', headless: true });
    try {
        const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });
        const errors = [];
        page.on('pageerror', error => errors.push(error.message));
        await page.route('**/*', route => {
            const url = new URL(route.request().url());
            if (url.origin === 'http://localhost:8080' && !url.pathname.includes('jsonrpc')) {
                return route.continue();
            }
            return route.abort();
        });
        await page.addInitScript(() => {
            window.WebSocket = class { constructor() { throw new Error('Test blocks WebSocket'); } };
        });
        await page.goto('http://localhost:8080/#!/downloading', { waitUntil: 'networkidle' });
        await page.waitForSelector('#task-table');
        const results = await page.evaluate(() => {
            const injector = angular.element(document.body).injector();
            const root = injector.get('$rootScope');
            const service = injector.get('aria2TaskService');
            const results = [];
            for (const size of [25, 250]) {
                const tasks = Array.from({ length: size }, (_, index) => ({
                    gid: String(index), status: 'active', totalLength: '10000',
                    completedLength: '2500', downloadSpeed: '100', uploadSpeed: '0',
                    connections: '1', files: [{ index: '1', path: '/synthetic/file-' + index,
                        length: '10000', completedLength: '2500', selected: 'true', uris: [] }]
                }));
                service.processDownloadTasks(tasks);
                root.$apply(() => { root.taskContext.list = tasks; });
                const firstRow = document.querySelector('.task-table-body > .row');
                const samples = [];
                for (let i = 0; i < 10; i++) {
                    const start = performance.now();
                    root.$apply(() => {
                        root.taskContext.list = tasks.map(task => angular.copy(task));
                    });
                    samples.push(performance.now() - start);
                }
                if (firstRow !== document.querySelector('.task-table-body > .row')) {
                    throw new Error('Full refresh replaced a stable task row');
                }
                const rowCount = document.querySelectorAll('.task-table-body > .row').length;
                if (rowCount !== size) { throw new Error('Unexpected task count'); }
                root.$apply(() => { root.searchContext.text = 'file-2'; });
                if (!document.querySelector('.task-table-body > .row')) {
                    throw new Error('Search lost matching rows');
                }
                root.$apply(() => { root.searchContext.text = ''; });
                samples.sort((a, b) => a - b);
                results.push({ tasks: size, refreshMedianMs: samples[5], refreshMaxMs: samples[9] });
            }
            return results;
        });
        await page.setViewportSize({ width: 375, height: 812 });
        assert.equal(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth), true);
        assert.deepEqual(errors, []);
        console.log(JSON.stringify({ browser: await browser.version(), results }));
        console.log('Stable rows, synthetic task counts, search, mobile width, and browser error checks passed.');
    } finally {
        await browser.close();
    }
})().catch(error => { console.error(error); process.exitCode = 1; });
