'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { chromium } = require(path.resolve('.local/browser-tools/node_modules/playwright'));

(async () => {
    // Disposable local Compose token only; never print settings or browser storage.
    const secret = Buffer.from(fs.readFileSync('.local/aria2/rpc-secret', 'utf8').trim()).toString('base64');
    const browser = await chromium.launch({ channel: 'msedge', headless: true });
    try {
        for (const protocol of ['http', 'ws']) {
            const context = await browser.newContext();
            try {
                await context.addInitScript(settings => {
                    localStorage.setItem('AriaNg.Options', JSON.stringify(settings));
                }, { rpcHost: '127.0.0.1', rpcPort: '8080', rpcInterface: 'jsonrpc',
                    protocol, httpMethod: 'POST', language: 'en', secret,
                    extendRpcServers: [], globalStatRefreshInterval: 1000,
                    downloadTaskRefreshInterval: 1000 });
                await context.route('**/*', route => route.request().url().startsWith('http://127.0.0.1:8080/')
                    ? route.continue() : route.abort());
                await context.routeWebSocket('**/*', socket => {
                    if (socket.url() === 'ws://127.0.0.1:8080/jsonrpc') {
                        socket.connectToServer();
                    } else {
                        socket.close();
                    }
                });
                const page = await context.newPage();
                const errors = [];
                page.on('pageerror', () => errors.push('uncaught browser error'));
                await page.goto('http://127.0.0.1:8080/#!/downloading', { waitUntil: 'domcontentloaded' });
                await page.waitForFunction(() => {
                    const injector = window.angular && angular.element(document.body).injector();
                    return injector && injector.get('ariaNgMonitorService').getCurrentGlobalStat().downloadSpeed !== undefined;
                });
                const success = await page.evaluate(() => new Promise(resolve => {
                    angular.element(document.body).injector().get('aria2RpcService').getVersion({
                        silent: true, callback: response => resolve(response.success)
                    });
                }));
                assert.equal(success, true);
                assert.deepEqual(errors, []);
                console.log('PASS application RPC transport: ' + protocol);
            } finally {
                await context.close();
            }
        }
    } finally {
        await browser.close();
    }
})().catch(() => {
    // Avoid serializing request/settings objects from an integration failure.
    console.error('Application RPC browser check failed; inspect the local fixture without exposing its token.');
    process.exitCode = 1;
});
