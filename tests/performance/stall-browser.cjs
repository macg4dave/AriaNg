'use strict';

const assert = require('node:assert/strict');
const path = require('node:path');
const { performance } = require('node:perf_hooks');
const { chromium } = require(path.resolve('.local/browser-tools/node_modules/playwright'));

// Browser automation belongs in JS. All RPC is intercepted: no credentials or
// upstream configuration are read, and only the local static UI is requested.
const origin = 'http://127.0.0.1:8080';
const percentile = (values, fraction) => values.length
    ? [...values].sort((a, b) => a - b)[Math.ceil(values.length * fraction) - 1] : null;

async function run(browser, protocol, tasks, files, delayMs) {
    const context = await browser.newContext();
    const pending = new Set();
    const durations = [];
    let bytes = 0;
    let listInFlight = 0;
    let maxListInFlight = 0;
    let listReplies = 0;
    let listRequests = 0;
    let unsupported = 0;
    let browserErrors = 0;
    const inventory = Array.from({ length: tasks }, (_, index) => ({
        gid: index.toString(16).padStart(16, '0'), status: 'active',
        totalLength: '100000', completedLength: '25000', downloadSpeed: '100',
        uploadSpeed: '0', connections: '1', numPieces: '100', pieceLength: '1000',
        dir: '/synthetic', files: Array.from({ length: files }, (_, file) => ({
            index: String(file + 1), path: '/synthetic/task-' + index + '/file-' + file,
            length: '1000', completedLength: '250', selected: 'true', uris: []
        }))
    }));
    function result(method, params) {
        if (method === 'aria2.tellActive') {
            listRequests++;
            const keys = params.find(Array.isArray);
            return inventory.map(task => Object.fromEntries(Object.entries({ ...task,
                completedLength: String(25000 + listRequests) })
                .filter(([key]) => !keys || keys.includes(key))));
        }
        if (method === 'aria2.getGlobalStat') {
            return { downloadSpeed: String(tasks * 100), uploadSpeed: '0',
                numActive: String(tasks), numWaiting: '0', numStopped: '0', numStoppedTotal: '0' };
        }
        if (method === 'aria2.getVersion') { return { version: '1.37.0', enabledFeatures: [] }; }
        if (method === 'aria2.getGlobalOption') { return {}; }
        if (method === 'system.multicall') {
            return params[0].map(call => [result(call.methodName, call.params || [])]);
        }
        unsupported++;
        return null;
    }
    function respond(request, send) {
        const start = performance.now();
        const list = request.method === 'aria2.tellActive';
        if (list) {
            listInFlight++;
            maxListInFlight = Math.max(maxListInFlight, listInFlight);
        }
        return new Promise((resolve, reject) => {
            const timer = setTimeout(async () => {
                pending.delete(timer);
                try {
                    const body = JSON.stringify({ jsonrpc: '2.0', id: request.id,
                        result: result(request.method, request.params || []) });
                    await send(body);
                    bytes += Buffer.byteLength(body);
                    if (list) {
                        listInFlight--;
                        listReplies++;
                        durations.push(performance.now() - start);
                    }
                    resolve();
                } catch (error) { reject(error); }
            }, delayMs);
            pending.add(timer);
        });
    }
    try {
        await context.addInitScript(settings => {
            localStorage.setItem('AriaNg.Options', JSON.stringify(settings));
            window.stallMetrics = { longTasks: [], digestMs: [], frameGaps: [], updateGaps: [], updates: 0 };
            new PerformanceObserver(list => {
                window.stallMetrics.longTasks.push(...list.getEntries().map(entry => entry.duration));
            }).observe({ type: 'longtask' });
        }, { rpcHost: '127.0.0.1', rpcPort: '8080', rpcInterface: 'jsonrpc',
            protocol, httpMethod: 'POST', language: 'en', secret: '', extendRpcServers: [],
            globalStatRefreshInterval: 1000, downloadTaskRefreshInterval: 1000 });
        await context.route('**/*', async route => {
            const url = new URL(route.request().url());
            if (url.origin !== origin) { return route.abort(); }
            if (url.pathname === '/jsonrpc') {
                return respond(route.request().postDataJSON(), body => route.fulfill({
                    contentType: 'application/json', body
                }));
            }
            return route.continue();
        });
        await context.routeWebSocket('**/*', socket => {
            if (socket.url() !== 'ws://127.0.0.1:8080/jsonrpc') { socket.close(); return; }
            socket.onMessage(message => {
                respond(JSON.parse(String(message)), body => socket.send(body))
                    .catch(() => { browserErrors++; });
            });
        });
        const page = await context.newPage();
        page.on('pageerror', () => { browserErrors++; });
        await page.goto(origin + '/#!/downloading', { waitUntil: 'domcontentloaded' });
        await page.waitForFunction(() => window.angular && angular.element(document.body).injector());
        await page.evaluate(() => {
            const root = angular.element(document.body).injector().get('$rootScope');
            let lastUpdate;
            root.$watch(() => root.taskContext.list && root.taskContext.list[0]
                && root.taskContext.list[0].completedLength, value => {
                if (!value) { return; }
                const now = performance.now();
                if (lastUpdate !== undefined) { window.stallMetrics.updateGaps.push(now - lastUpdate); }
                lastUpdate = now;
                window.stallMetrics.updates++;
            });
            const original = root.$digest;
            root.$digest = function () {
                const start = performance.now();
                try { return original.apply(this, arguments); }
                finally { window.stallMetrics.digestMs.push(performance.now() - start); }
            };
            let last;
            function frame(now) {
                if (last !== undefined) { window.stallMetrics.frameGaps.push(now - last); }
                last = now;
                requestAnimationFrame(frame);
            }
            requestAnimationFrame(frame);
        });
        await page.waitForFunction(count => document.querySelectorAll('.task-table-body > .row').length === count,
            tasks, { timeout: 10000 });
        // Four completed list replies include initial full data and basic refreshes.
        const deadline = performance.now() + 15000;
        while (listReplies < 4 && performance.now() < deadline) {
            await page.waitForTimeout(100);
        }
        assert.ok(listReplies >= 4, 'List did not continue refreshing');
        await page.waitForFunction(() => window.stallMetrics.updates >= 4, null, { timeout: 5000 });
        const metrics = await page.evaluate(() => window.stallMetrics);
        assert.equal(browserErrors, 0, 'Uncaught browser/transport errors');
        assert.equal(unsupported, 0, 'Fixture received an unsupported RPC method');
        assert.equal(maxListInFlight, 1, 'List polls overlapped');
        return { protocol, tasks, files, delayMs, listReplies, responseBytes: bytes,
            fixtureReplyP95Ms: percentile(durations, 0.95), maxListInFlight, browserErrors,
            observedUpdates: metrics.updates, updateGapMaxMs: Math.max(0, ...metrics.updateGaps),
            digestP95Ms: percentile(metrics.digestMs, 0.95),
            frameGapP95Ms: percentile(metrics.frameGaps, 0.95),
            frameGapMaxMs: Math.max(0, ...metrics.frameGaps),
            longTasks: metrics.longTasks.length, longTaskMaxMs: Math.max(0, ...metrics.longTasks) };
    } finally {
        for (const timer of pending) { clearTimeout(timer); }
        await context.close();
    }
}

(async () => {
    const browser = await chromium.launch({ channel: 'msedge', headless: true });
    try {
        console.log(JSON.stringify({ browser: await browser.version(), fixture: 'synthetic intercepted RPC',
            pollIntervalMs: 1000, clients: 1 }));
        for (const protocol of ['http', 'ws']) {
            for (const [tasks, files, delay] of [[20, 1, 0], [25, 1, 0], [25, 100, 0], [25, 100, 1500]]) {
                console.log(JSON.stringify(await run(browser, protocol, tasks, files, delay)));
            }
        }
    } finally { await browser.close(); }
})().catch(() => {
    console.error('Stall diagnostic failed; request bodies and browser settings omitted.');
    process.exitCode = 1;
});
