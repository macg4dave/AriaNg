'use strict';

const assert = require('node:assert/strict');
const path = require('node:path');
const crypto = require('node:crypto');
const { chromium } = require(path.resolve('.local/browser-tools/node_modules/playwright'));

const base = process.env.GATEWAY_FIXTURE_ORIGIN || 'http://127.0.0.1:8080';
const endpoint = process.env.GATEWAY_FIXTURE_URL;
const token = process.env.GATEWAY_FIXTURE_TOKEN;
const upstreamToken = process.env.ARIA2_FIXTURE_TOKEN;
let stage = 'startup';

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
        const page = await rpc('gateway.listTasks', [{ source: 'waiting', search: 'gateway-fixture-', limit: 10 }]);
        assert.equal(page.tasks.length, 1);
        assert.equal(page.tasks[0].gid, gid);
        assert.equal(page.tasks[0].fileCount, 1);
        assert.ok(!Object.hasOwn(page.tasks[0], 'files'));
        const same = await rpc('gateway.listTasks', [{ source: 'waiting', search: 'gateway-fixture-', snapshot: page.snapshot }]);
        assert.equal(same.tasks[0].gid, gid);
        for (let i = 0; i < 2; i++) {
            stage = 'client ' + i + ' startup';
            const context = await browser.newContext();
            contexts.push(context);
            await context.addInitScript(settings => {
                localStorage.setItem('AriaNg.Options', JSON.stringify(settings));
            }, { rpcHost: new URL(base).hostname, rpcPort: new URL(base).port, rpcInterface: 'v1/servers/fixture/rpc',
                protocol: 'http', httpMethod: 'POST', rpcMode: 'gateway', secret: Buffer.from(token).toString('base64'),
                rpcRequestHeaders: '',
                language: 'en', extendRpcServers: [], globalStatRefreshInterval: 1000,
                downloadTaskRefreshInterval: 1000 });
            // Stand in for the production same-origin reverse proxy while exercising
            // the actual gateway HTTP listener and real disposable aria2.
            await context.route(base + '/v1/servers/fixture/rpc', async route => {
                requests++;
                assert.ok(!route.request().postData().includes(upstreamToken));
                assert.ok(!route.request().postData().includes(token));
                assert.ok(route.request().headers().authorization === 'Bearer ' + token, 'Browser credential forwarding failed');
                if (process.env.GATEWAY_FIXTURE_PROXY === '1') {
                    return route.continue();
                }
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
            stage = 'client ' + i + ' compact list';
            await page.waitForFunction(expected => document.body.textContent.includes(expected), 'gateway-fixture-');
            stage = 'client ' + i + ' summary file count';
            assert.equal(await page.locator('.task-files a').first().innerText(), '(1 Files)');
            assert.equal(await page.locator('.task-table-body > .row').count(), 1);
            stage = 'client ' + i + ' version';
            const version = await page.evaluate(() => new Promise(resolve => {
                angular.element(document.body).injector().get('aria2RpcService').getVersion({
                    silent: true, callback: response => resolve(response.success)
                });
            }));
            assert.equal(version, true);
            stage = 'client ' + i + ' task detail';
            await page.locator('.task-files a').first().click();
            await page.waitForFunction(expected => document.body.textContent.includes(expected), 'gateway-fixture-');
            await page.waitForFunction(expected => location.hash === '#!/task/detail/' + expected, gid);
            await page.locator('a[href="#!/settings/ariang"]').click();
            stage = 'client ' + i + ' AriaNg settings';
            await page.locator('select[ng-model="context.settings.theme"]').waitFor();
            await page.locator('.sidebar .treeview > a').click();
            for (const settings of ['basic', 'http-ftp-sftp', 'http', 'ftp-sftp', 'bt', 'metalink', 'rpc', 'advanced']) {
                stage = 'client ' + i + ' aria2 settings ' + settings;
                await page.locator('a[href="#!/settings/aria2/' + settings + '"]').click();
                await page.waitForFunction(expected => location.hash === '#!/settings/aria2/' + expected, settings);
                await page.locator('.settings-table').first().waitFor();
            }
            await page.locator('a[href="#!/status"]').click();
            await page.waitForFunction(() => location.hash === '#!/status');
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
})().catch(error => {
    console.error('Gateway browser fixture failed at ' + stage + ': ' + error.name);
    process.exitCode = 1;
});
