'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { performance } = require('node:perf_hooks');
const { chromium } = require(path.resolve('.local/browser-tools/node_modules/playwright'));

// Only the disposable Compose fixture is supported. Never accept remote URLs or
// print request bodies, backend errors, browser storage, or connection settings.
const origin = 'http://127.0.0.1:8080';
const token = fs.readFileSync('.local/aria2/rpc-secret', 'utf8').trim();
let stage = 'startup';
const percentile = (values, fraction) => [...values].sort((a, b) => a - b)
    [Math.ceil(values.length * fraction) - 1] ?? null;

async function rpc(method, ...params) {
    const response = await fetch(origin + '/jsonrpc', {
        method: 'POST', signal: AbortSignal.timeout(10000),
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ jsonrpc: '2.0', id: 'fixture', method,
            params: ['token:' + token, ...params] })
    });
    const body = await response.json();
    if (!response.ok || body.error) { throw new Error('Local fixture RPC failed'); }
    return body.result;
}

async function observe(browser, protocol, totalTasks, activeTasks) {
    stage = protocol + ' list startup';
    const context = await browser.newContext();
    const requests = new Map();
    const samples = [];
    const sockets = new Set();
    let responseBytes = 0;
    let browserErrors = 0;
    let outage = false;
    let listReplies = 0;
    let detailReplies = 0;
    let failedRequests = 0;
    function sent(body) {
        const request = JSON.parse(body);
        requests.set(request.id, { start: performance.now(), method: request.method });
    }
    function received(body) {
        const response = JSON.parse(body);
        responseBytes += Buffer.byteLength(body);
        const request = requests.get(response.id);
        if (!request) { return; }
        requests.delete(response.id);
        if (response.error) { failedRequests++; return; }
        samples.push(performance.now() - request.start);
        if (request.method === 'aria2.tellWaiting') { listReplies++; }
        if (request.method === 'system.multicall') { detailReplies++; }
    }
    try {
        await context.addInitScript(settings => {
            localStorage.setItem('AriaNg.Options', JSON.stringify(settings));
            window.liveMetrics = { longTasks: [], frameGaps: [] };
            new PerformanceObserver(list => {
                window.liveMetrics.longTasks.push(...list.getEntries().map(item => item.duration));
            }).observe({ type: 'longtask' });
            let previous;
            function frame(now) {
                if (previous !== undefined) { window.liveMetrics.frameGaps.push(now - previous); }
                previous = now;
                requestAnimationFrame(frame);
            }
            requestAnimationFrame(frame);
        }, { rpcHost: '127.0.0.1', rpcPort: '8080', rpcInterface: 'jsonrpc', protocol,
            httpMethod: 'POST', language: 'en', secret: Buffer.from(token).toString('base64'),
            extendRpcServers: [], globalStatRefreshInterval: 1000,
            downloadTaskRefreshInterval: 1000, webSocketReconnectInterval: 0 });
        await context.route('**/*', async route => {
            const url = new URL(route.request().url());
            if (url.origin !== origin) { return route.abort(); }
            if (url.pathname !== '/jsonrpc') { return route.continue(); }
            if (outage) { failedRequests++; return route.abort(); }
            sent(route.request().postData());
            const response = await route.fetch({ timeout: 10000 });
            received(await response.text());
            return route.fulfill({ response });
        });
        await context.routeWebSocket('**/*', socket => {
            if (socket.url() !== 'ws://127.0.0.1:8080/jsonrpc') { socket.close(); return; }
            sockets.add(socket);
            const server = socket.connectToServer();
            socket.onMessage(body => { sent(String(body)); server.send(body); });
            server.onMessage(body => { received(String(body)); socket.send(body); });
            socket.onClose(() => { sockets.delete(socket); server.close(); });
            server.onClose(() => { sockets.delete(socket); socket.close(); });
        });
        const page = await context.newPage();
        page.on('pageerror', () => { browserErrors++; });
        await page.goto(origin + '/#!/waiting', { waitUntil: 'domcontentloaded' });
        await page.waitForFunction(count => document.querySelectorAll('.task-table-body > .row').length === count,
            totalTasks - activeTasks);
        const deadline = performance.now() + 10000;
        while (listReplies < 4 && performance.now() < deadline) { await page.waitForTimeout(100); }
        assert.ok(listReplies >= 4, 'Live list polling stalled');
        const baseline = { replies: listReplies, rpcP95Ms: percentile(samples, 0.95),
            rpcP99Ms: percentile(samples, 0.99), responseBytes };

        // Interrupt only this browser's traffic; the upstream keeps transferring.
        stage = protocol + ' disconnect';
        outage = true;
        if (protocol === 'ws') {
            for (const socket of sockets) { socket.close({ code: 1011, reason: 'Fixture outage' }); }
        }
        await page.waitForFunction(() => angular.element(document.body).injector()
            .get('$rootScope').taskContext.rpcStatus === 'Disconnected', null, { timeout: 5000 });
        const beforeRecovery = listReplies;
        stage = protocol + ' recovery';
        requests.clear();
        outage = false;
        if (protocol === 'ws') {
            await page.evaluate(() => angular.element(document.body).injector()
                .get('aria2RpcService').reconnect({ silent: true }));
        }
        await page.waitForFunction(() => angular.element(document.body).injector()
            .get('$rootScope').taskContext.rpcStatus === 'Connected', null, { timeout: 10000 });
        const recoveryDeadline = performance.now() + 5000;
        while (listReplies <= beforeRecovery && performance.now() < recoveryDeadline) {
            await page.waitForTimeout(100);
        }
        assert.ok(listReplies > beforeRecovery, 'List did not recover');
        assert.equal(await page.locator('.task-table-body > .row').count(), totalTasks - activeTasks);
        stage = protocol + ' detail navigation';
        const detailRoute = await page.locator('.task-table-body a[href*="/task/detail/"]').first().getAttribute('href');
        // Navigate the Angular route directly; this measures refresh behavior,
        // independently of modal overlays and pointer interaction.
        await page.evaluate(hash => { window.location.hash = hash; }, detailRoute);
        const detailDeadline = performance.now() + 10000;
        while (detailReplies < 2 && performance.now() < detailDeadline) {
            await page.waitForTimeout(100);
        }
        assert.ok(detailReplies >= 2, 'Live detail refresh stalled');
        assert.equal(browserErrors, 0, 'Uncaught live browser error');
        const metrics = await page.evaluate(() => window.liveMetrics);
        return { protocol, totalTasks, activeTasks, waitingTasks: totalTasks - activeTasks,
            baseline, detailReplies, failedRequests, injectedDisconnects: 1,
            disconnectedObserved: true, recovered: true, browserErrors,
            longTasks: metrics.longTasks.length, longTaskMaxMs: Math.max(0, ...metrics.longTasks),
            frameGapP95Ms: percentile(metrics.frameGaps, 0.95),
            frameGapMaxMs: Math.max(0, ...metrics.frameGaps) };
    } finally { await context.close(); }
}

(async () => {
    const browser = await chromium.launch({ channel: 'msedge', headless: true });
    const gids = [];
    try {
        const initial = await rpc('aria2.getGlobalStat');
        assert.equal(Number(initial.numActive) + Number(initial.numWaiting), 0,
            'Disposable fixture must have no existing active or waiting tasks');
        console.log(JSON.stringify({ browser: await browser.version(), fixture: 'disposable aria2',
            clients: 1, pollIntervalMs: 1000, filesPerTask: 1, peers: 0 }));
        for (let index = 0; index < 25; index++) {
            gids.push(await rpc('aria2.addUri', ['http://web:8080/test-fixture.bin'], {
                pause: 'true', out: 'stall-' + Date.now() + '-' + index + '.bin',
                'max-download-limit': '1K', 'allow-overwrite': 'false'
            }));
            if (index === 19) {
                for (const protocol of ['http', 'ws']) {
                    console.log(JSON.stringify(await observe(browser, protocol, 20, 0)));
                }
            }
        }
        for (const gid of gids.slice(0, 4)) { await rpc('aria2.unpause', gid); }
        const activeDeadline = performance.now() + 5000;
        while (Number((await rpc('aria2.getGlobalStat')).numActive) !== 4 && performance.now() < activeDeadline) {
            await new Promise(resolve => setTimeout(resolve, 100));
        }
        for (const protocol of ['http', 'ws']) {
            assert.equal(Number((await rpc('aria2.getGlobalStat')).numActive), 4,
                'Fixture must have four active transfers');
            console.log(JSON.stringify(await observe(browser, protocol, 25, 4)));
        }
        for (const gid of gids.slice(0, 4)) {
            const task = await rpc('aria2.tellStatus', gid, ['completedLength']);
            assert.ok(Number(task.completedLength) > 0, 'Active fixture transfer made no progress');
        }
    } finally {
        let cleanupFailures = 0;
        for (const gid of gids) {
            try {
                const task = await rpc('aria2.tellStatus', gid, ['status']);
                if (!['complete', 'error', 'removed'].includes(task.status)) {
                    await rpc('aria2.forceRemove', gid);
                }
            } catch (_) { cleanupFailures++; }
            // A never-started task can disappear immediately after removal.
            try { await rpc('aria2.removeDownloadResult', gid); } catch (_) { /* Already absent. */ }
        }
        try {
            const remaining = await rpc('aria2.tellStopped', 0, 1000, ['gid']);
            if (remaining.some(task => gids.includes(task.gid))) { cleanupFailures++; }
        } catch (_) { cleanupFailures++; }
        await browser.close();
        assert.equal(cleanupFailures, 0, 'Fixture cleanup failed');
    }
})().catch(() => {
    console.error('Live stall diagnostic failed at ' + stage + '; private fixture details omitted.');
    process.exitCode = 1;
});
