'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const { execFileSync } = require('node:child_process');
const { chromium } = require(path.resolve('.local/browser-tools/node_modules/playwright'));
const files = ['src/scripts/directives/chart.js', 'src/scripts/services/ariaNgMonitorService.js'];

(async () => {
    const browser = await chromium.launch({ channel: 'msedge', headless: true });
    try {
        const page = await browser.newPage();
        await page.route('**/*', route => {
            const url = new URL(route.request().url());
            return url.origin === 'http://127.0.0.1:8080' && url.pathname !== '/jsonrpc'
                ? route.continue() : route.abort();
        });
        await page.routeWebSocket('**/*', socket => socket.close());
        await page.goto('http://127.0.0.1:8080/#!/downloading');
        const results = [];
        for (const baseline of [true, false]) {
            const sources = files.map(file => baseline
                ? execFileSync('git', ['show', 'HEAD:' + file], { encoding: 'utf8' })
                : fs.readFileSync(file, 'utf8'));
            results.push(await page.evaluate(({ sources, baseline }) => {
                const name = baseline ? 'chartBaseline' : 'chartUpdated';
                const module = angular.module(name, []);
                module.filter('readableVolume', () => angular.element(document.body).injector().get('$filter')('readableVolume'));
                let server = 'synthetic-A';
                module.value('ariaNgConstants', { globalStatStorageCapacity: 120,
                    taskStatStorageCapacity: 300, taskStatHistoryCapacity: 32 });
                module.value('ariaNgCommonService', { getCurrentUnixTime: () => Date.now() });
                module.value('ariaNgLocalizationService', { getLocalizedText: value => value });
                module.value('ariaNgSettingService', { getCurrentRpcSessionId: () => server });
                for (const source of sources) {
                    new Function(source.replace("angular.module('ariaNg')", "angular.module('" + name + "')"))();
                }
                const injector = angular.injector(['ng', name]);
                const root = injector.get('$rootScope');
                const compile = injector.get('$compile');
                const monitor = injector.get('ariaNgMonitorService');
                const events = () => (($._data(window, 'events') || {}).resize || []).map(item => item.handler);
                const initialHandlers = events();
                const retained = [];
                for (let index = 0; index < 100; index++) {
                    retained.push(monitor.getStatsData('task-' + index));
                }
                const retainedHistoryCount = retained.filter((data, index) => monitor.getStatsData('task-' + (99 - index)) === retained[99 - index]).length;
                const data = monitor.getEmptyStatsData('bench');
                for (let index = 0; index < 500; index++) {
                    monitor.recordStat('bench', { downloadSpeed: index, uploadSpeed: index / 2 });
                }
                if (data.series[0].data.length !== 300 || data.xAxis.data.length !== 300) {
                    throw new Error('Sample capacity changed');
                }
                let disposed = 0;
                const originalInit = echarts.init;
                echarts.init = function () {
                    const chart = originalInit.apply(this, arguments);
                    const originalDispose = chart.dispose;
                    chart.dispose = function () { disposed++; return originalDispose.apply(this, arguments); };
                    return chart;
                };
                let digestMs;
                try {
                    for (let index = 0; index < 20; index++) {
                        const scope = root.$new();
                        scope.data = data;
                        scope.theme = index % 2 ? 'dark' : 'light';
                        const host = angular.element('<div style="width:600px;height:200px"></div>').appendTo(document.body);
                        const chart = compile('<ng-chart ng-data="data" ng-theme="theme" height="200"></ng-chart>')(scope).appendTo(host);
                        scope.$digest();
                        if (index === 0) {
                            const samples = [];
                            for (let run = 0; run < 5; run++) {
                                const start = performance.now();
                                for (let tick = 0; tick < 500; tick++) { scope.$digest(); }
                                samples.push(performance.now() - start);
                            }
                            samples.sort((a, b) => a - b);
                            digestMs = samples[2];
                            monitor.recordStat('bench', { downloadSpeed: 999, uploadSpeed: 123 });
                            scope.$digest();
                            const instance = echarts.getInstanceByDom(chart.find('div')[0]);
                            if (instance.getOption().series[1].data.at(-1) !== 999) {
                                throw new Error('Chart did not consume new sample');
                            }
                        }
                        scope.$destroy();
                        host.remove();
                    }
                    const scope = root.$new();
                    scope.data = data;
                    const button = compile('<button ng-pop-chart ng-data="data">fixture</button>')(scope).appendTo(document.body);
                    scope.$digest();
                    const popover = button.data('bs.popover');
                    popover.options.animation = false;
                    button.popover('show');
                    scope.$destroy();
                    const popoverDestroyed = !button.data('bs.popover');
                    if (!popoverDestroyed) {
                        const instance = echarts.getInstanceByDom(popover.tip().find('.chart-pop')[0]);
                        if (instance) { instance.dispose(); }
                        button.popover('destroy');
                    }
                    button.remove();
                    const extraHandlers = events().filter(handler => !initialHandlers.includes(handler));
                    for (const handler of extraHandlers) { angular.element(window).off('resize', handler); }
                    const previous = monitor.getStatsData('server-isolation');
                    for (let index = 0; index < 500; index++) {
                        monitor.recordGlobalStat({ downloadSpeed: 456, uploadSpeed: 0 });
                    }
                    if (monitor.getGlobalStatsData().series[0].data.length !== 120) {
                        throw new Error('Global sample capacity changed');
                    }
                    server = 'synthetic-B';
                    const isolated = monitor.getStatsData('server-isolation') !== previous
                        && monitor.getCurrentGlobalStat().downloadSpeed === undefined;
                    return { baseline, navigationCycles: 20, retainedHistoryCount,
                        retainedResizeHandlers: extraHandlers.length, disposedCharts: disposed,
                        popoverDestroyed, serverIsolated: isolated, quietDigests: 500, digestMedianMs: digestMs };
                } finally {
                    echarts.init = originalInit;
                    root.$destroy();
                }
            }, { sources, baseline }));
        }
        assert.equal(results[1].retainedResizeHandlers, 0);
        assert.equal(results[1].popoverDestroyed, true);
        assert.equal(results[1].serverIsolated, true);
        assert.ok(results[1].retainedHistoryCount <= 32);
        assert.ok(results[1].disposedCharts >= 21);
        console.log(JSON.stringify({ browser: await browser.version(), results }));
    } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
