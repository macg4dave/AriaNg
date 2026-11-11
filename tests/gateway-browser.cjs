'use strict';

const assert = require('node:assert/strict');
const path = require('node:path');
const crypto = require('node:crypto');
const { chromium } = require(path.resolve('.local/browser-tools/node_modules/playwright'));

const base = 'http://127.0.0.1:8080';
const endpoint = process.env.GATEWAY_FIXTURE_URL;
const token = process.env.GATEWAY_FIXTURE_TOKEN;
const upstreamToken = process.env.ARIA2_FIXTURE_TOKEN;

(async () => {
    assert.ok(endpoint && token && upstreamToken, 'Run through the Rust fixture launcher');
    const browser = await chromium.launch({ channel: 'msedge', headless: true });
    let gid;
    const contexts = [];
    let requests = 0;
    const rpc = async (method, params = []) => {
        const response = await fetch(endpoint, { method: 'POST',
            headers: { Authorization: 'Bearer ' + token, 'Content-Type': 'application/json', Origin: base },
            body: JSON.stringify({ jsonrpc: '2.0', id: 'fixture', method, params }) });
        const body = await response.json();
        assert.ok(response.ok && !body.error, 'Gateway RPC operation failed');
        return body.result;
    };
    try {
        gid = await rpc('aria2.addUri', [['http://web:8080/test-fixture.bin'], {
            pause: 'true', out: 'gateway-fixture-' + crypto.randomUUID() + '.bin'
        }]);
        for (let i = 0; i < 2; i++) {
            const context = await browser.newContext();
            contexts.push(context);
            await context.addInitScript(settings => {
                localStorage.setItem('AriaNg.Options', JSON.stringify(settings));
            }, { rpcHost: '127.0.0.1', rpcPort: '8080', rpcInterface: 'v1/servers/fixture/rpc',
                protocol: 'http', httpMethod: 'POST', secret: '',
                rpcRequestHeaders: 'Authorization: Bearer ' + token,
                language: 'en', extendRpcServers: [], globalStatRefreshInterval: 1000,
                downloadTaskRefreshInterval: 1000 });
            // Stand in for the production same-origin reverse proxy while exercising
            // the actual gateway HTTP listener and real disposable aria2.
            await context.route(base + '/v1/servers/fixture/rpc', async route => {
                requests++;
                assert.ok(!route.request().postData().includes(upstreamToken));
                const response = await context.request.post(endpoint, {
                    headers: { ...route.request().headers(), origin: base },
                    data: route.request().postData()
                });
                assert.ok(!(await response.text()).includes(upstreamToken));
                await route.fulfill({ response });
            });
            const page = await context.newPage();
            const errors = [];
            page.on('pageerror', () => errors.push('uncaught browser error'));
            await page.goto(base + '/#!/waiting', { waitUntil: 'domcontentloaded' });
            await page.waitForFunction(expected => document.body.textContent.includes(expected), 'gateway-fixture-');
            const version = await page.evaluate(() => new Promise(resolve => {
                angular.element(document.body).injector().get('aria2RpcService').getVersion({
                    silent: true, callback: response => resolve(response.success)
                });
            }));
            assert.equal(version, true);
            await page.goto(base + '/#!/task/detail/' + gid, { waitUntil: 'domcontentloaded' });
            await page.waitForFunction(expected => document.body.textContent.includes(expected), 'gateway-fixture-');
            await page.setViewportSize({ width: 390, height: 844 });
            assert.deepEqual(errors, []);
        }
        const result = await rpc('system.multicall', [[
            { methodName: 'aria2.tellStatus', params: [gid] },
            { methodName: 'aria2.getGlobalStat', params: [] }
        ]]);
        assert.equal(result[0][0].gid, gid);
        assert.ok(result[1][0].numActive !== undefined);
        assert.ok(!JSON.stringify(await rpc('aria2.getGlobalOption')).includes(upstreamToken));
        await rpc('aria2.changeOption', [gid, { 'max-download-limit': '8K' }]);
        assert.equal((await rpc('aria2.getOption', [gid]))['max-download-limit'], '8192');
        const unauthorized = await fetch(endpoint, { method: 'POST',
            headers: { Authorization: 'Bearer wrong', 'Content-Type': 'application/json' },
            body: JSON.stringify({ jsonrpc: '2.0', id: 1, method: 'aria2.getVersion' }) });
        assert.equal(unauthorized.status, 401);
        console.log('PASS gateway: two browser clients, waiting/detail/mobile routes, multicall, task options, auth; ' + requests + ' browser RPCs');
    } finally {
        for (const context of contexts) {
            await context.close();
        }
        await browser.close();
        if (gid) {
            await rpc('aria2.remove', [gid]);
        }
    }
})().catch(() => {
    console.error('Gateway browser fixture failed; inspect disposable services without exposing credentials.');
    process.exitCode = 1;
});
