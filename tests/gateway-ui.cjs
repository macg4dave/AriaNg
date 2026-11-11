'use strict';

const assert = require('node:assert/strict');
const path = require('node:path');
const { chromium } = require(path.resolve('.local/browser-tools/node_modules/playwright'));
const base = 'http://127.0.0.1:8080';

(async () => {
    const browser = await chromium.launch({ channel: 'msedge', headless: true });
    const context = await browser.newContext();
    let stage = 'startup';
    try {
        const inventory = Array.from({ length: 250 }, (_, i) => ({
            gid: i.toString(16).padStart(16, '0'), status: 'paused', taskName: 'fixture-' + String(i).padStart(3, '0'),
            totalLength: String(1000 + i), completedLength: '500', downloadSpeed: '0', uploadSpeed: '0',
            fileCount: 100, selectedFileCount: 100, position: i, singleUrl: 'http://fixture.invalid/file-' + i
        }));
        let expireNext = false;
        let slowNext = false;
        let inflight = 0;
        let maxInflight = 0;
        let queries = 0;
        let conflicts = 0;
        let moved;
        let unauthorized = false;
        let stageErrors = 0;
        await context.addInitScript(() => {
            localStorage.setItem('AriaNg.Options', JSON.stringify({ language: 'en', rpcHost: '127.0.0.1',
                rpcPort: '8080', rpcInterface: 'v1/servers/fixture/rpc', protocol: 'http', rpcMode: 'gateway',
                httpMethod: 'GET', secret: btoa('synthetic-browser-token'), globalStatRefreshInterval: 1000,
                downloadTaskRefreshInterval: 1000, extendRpcServers: [] }));
        });
        await context.route('**/*', async route => {
            const url = new URL(route.request().url());
            if (url.origin !== base) { return route.abort(); }
            if (url.pathname !== '/v1/servers/fixture/rpc') { return route.continue(); }
            assert.equal(route.request().method(), 'POST');
            assert.equal(route.request().headers().authorization, 'Bearer synthetic-browser-token');
            const body = route.request().postDataJSON();
            assert.ok(!JSON.stringify(body).includes('synthetic-browser-token'));
            const respond = (result, error, status = 200) => route.fulfill({ status, contentType: 'application/json',
                body: JSON.stringify({ jsonrpc: '2.0', id: body.id, ...(error ? { error } : { result }) }) });
            if (unauthorized) { return respond(null, { code: 1, message: 'Unauthorized' }, 401); }
            if (body.method === 'gateway.listTasks') {
                queries++;
                inflight++;
                maxInflight = Math.max(maxInflight, inflight);
                try {
                    const query = body.params[0];
                    if (slowNext) {
                        slowNext = false;
                        await new Promise(resolve => setTimeout(resolve, 1500));
                    }
                    if (expireNext && query.snapshot) {
                        expireNext = false;
                        conflicts++;
                        return await respond(null, { code: -32003, message: 'Snapshot expired' }, 409);
                    }
                    let tasks = inventory.filter(task => task.taskName.includes(query.search));
                    if (query.sort === 'name') { tasks = tasks.toSorted((a, b) => a.taskName.localeCompare(b.taskName)); }
                    if (query.reverse) { tasks = tasks.toReversed(); }
                    return await respond({ tasks: tasks.slice(query.offset, query.offset + query.limit), total: tasks.length,
                        sourceCount: inventory.length, snapshot: 'a'.repeat(64), offset: query.offset, limit: query.limit,
                        hasMore: query.offset + query.limit < tasks.length });
                } finally { inflight--; }
            }
            if (body.method === 'aria2.getGlobalStat') {
                return respond({ downloadSpeed: '0', uploadSpeed: '0', numActive: '0', numWaiting: '250', numStopped: '0' });
            }
            if (body.method === 'aria2.changePosition') {
                moved = body.params;
                return respond(body.params[1]);
            }
            if (body.method === 'aria2.getVersion') {
                return respond(null, { code: -1, message: '<b>synthetic error</b>' });
            }
            stageErrors++;
            return respond(null, { code: -32601, message: 'Unsupported fixture method' });
        });
        const page = await context.newPage();
        const errors = [];
        page.on('pageerror', () => errors.push('uncaught browser error'));
        const rows = page.locator('.task-table-body > .row');
        await page.goto(base + '/#!/waiting');
        await page.waitForFunction(() => document.querySelectorAll('.task-table-body > .row').length === 100);
        assert.ok((await rows.first().innerText()).includes('fixture-000'));
        assert.ok((await page.locator('.content-wrapper').innerText()).includes('Page 1 of 3 (250 tasks)'));
        stage = 'pagination and revision recovery';
        await rows.first().click();
        expireNext = true;
        await page.getByRole('button', { name: 'Next Page', exact: true }).click();
        await page.waitForFunction(() => document.querySelector('.task-table-body').textContent.includes('fixture-100'));
        assert.equal(conflicts, 1);
        assert.equal(await rows.count(), 100);
        assert.equal(await page.evaluate(() => angular.element(document.body).injector().get('$rootScope').taskContext.getSelectedTaskIds().length), 0);
        stage = 'queue position on second page';
        await page.evaluate(() => {
            const scope = angular.element(document.querySelector('.task-table-body')).scope();
            const row = document.querySelectorAll('.task-table-body > .row')[3];
            scope.$apply(() => scope.$emit('task-list.drop-model', row, row, row.parentElement));
        });
        await page.waitForFunction(() => !angular.element(document.querySelector('.task-table-body')).scope().pageState.loading);
        assert.equal(moved[1], 103);
        stage = 'search and sorting';
        await page.locator('#search-box').fill('fixture-249');
        await page.waitForFunction(() => document.querySelectorAll('.task-table-body > .row').length === 1);
        assert.ok((await rows.first().innerText()).includes('fixture-249'));
        await page.locator('#search-box').fill('');
        await page.waitForFunction(() => document.querySelectorAll('.task-table-body > .row').length === 100);
        await page.locator('.task-table-title a').first().click();
        await page.locator('.task-table-title a').first().click();
        await page.waitForFunction(() => document.querySelector('.task-table-body').textContent.includes('fixture-249'));
        assert.ok((await rows.first().innerText()).includes('fixture-249'));
        stage = 'slow requests and route cleanup';
        slowNext = true;
        await page.locator('#search-box').fill('fixture-1');
        await page.waitForTimeout(1100);
        await page.locator('a[href="#!/settings/ariang"]').click();
        await page.locator('select[ng-model="context.settings.theme"]').waitFor();
        await page.waitForTimeout(800);
        assert.equal(await page.evaluate(() => angular.element(document.body).injector().get('$rootScope').taskContext.list.length), 0);
        assert.equal(maxInflight, 1);
        stage = 'HTTP 200 RPC errors and escaped notifications';
        const succeeded = await page.evaluate(() => new Promise(resolve => {
            angular.element(document.body).injector().get('aria2RpcService').getVersion({
                callback: response => resolve(response.success)
            });
        }));
        assert.equal(succeeded, false);
        await page.locator('.ui-notification.error .message').filter({ hasText: '<b>synthetic error</b>' }).waitFor();
        assert.equal(await page.locator('.ui-notification.error .message b').count(), 0);
        stage = 'authorization failure and navigation';
        unauthorized = true;
        await page.locator('a[href="#!/waiting"]').click();
        await page.waitForTimeout(300);
        await page.locator('a[href="#!/settings/ariang"]').click();
        await page.locator('select[ng-model="context.settings.theme"]').waitFor();
        stage = 'profile persistence';
        assert.equal(await page.evaluate(() => {
            const settings = angular.element(document.body).injector().get('ariaNgSettingService');
            const profile = settings.addNewRpcSetting();
            if (profile.rpcMode !== 'direct') { return false; }
            profile.rpcMode = 'gateway';
            settings.updateRpcSetting(profile, 'rpcMode');
            settings.importAllOptions(settings.exportAllOptions());
            const reloaded = settings.getAllRpcSettings().find(value => value.rpcId === profile.rpcId);
            return reloaded && reloaded.rpcMode === 'gateway';
        }), true);
        assert.deepEqual(errors, []);
        assert.equal(stageErrors, 0);
        console.log(JSON.stringify({ fixture: '250 synthetic tasks, 100 files per summary', queries, maxInflight,
            conflicts, maximumRenderedRows: 100, errors: errors.length }));
        console.log('PASS gateway UI: pagination, server search/sort, selection reset, queue offset, stale revision, slow RPC, route cleanup and unauthorized navigation.');
    } catch (error) {
        console.error('Gateway UI check failed at ' + stage + ': ' + error.message);
        process.exitCode = 1;
    } finally {
        await context.close();
        await browser.close();
    }
})().catch(() => { console.error('Gateway UI fixture could not start.'); process.exitCode = 1; });
