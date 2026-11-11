'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const { execFileSync } = require('node:child_process');
const file = 'src/scripts/controllers/task-detail.js';
const noop = () => {};

function fixture(source) {
    let controller;
    let tick;
    let destroy;
    let cancelled = false;
    const pending = [];
    const root = { taskContext: { list: [] }, swipeActions: {} };
    const scope = { $on: (event, handler) => { if (event === '$destroy') { destroy = handler; } } };
    function request(kind, gid, callback) {
        let resolve;
        let reject;
        const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
        pending.push({ kind, callback, resolve, reject });
        return promise;
    }
    const interval = handler => { tick = handler; return 1; };
    interval.cancel = () => { cancelled = true; };
    const dependencies = {
        $rootScope: root, $scope: scope, $routeParams: { gid: 'synthetic' }, $interval: interval,
        aria2RpcErrors: { Unauthorized: { message: 'Unauthorized' } },
        ariaNgSettingService: { getDownloadTaskRefreshInterval: () => 1000,
            getShowPiecesInfoInTaskDetailPage: () => 'never' },
        ariaNgMonitorService: { getEmptyStatsData: () => ({}), recordStat: noop },
        ariaNgCommonService: { copyObjectTo: task => ({ ...task }), extendArray: () => false },
        aria2SettingService: { getAvailableTaskOptionKeys: () => [], getSpecifiedOptions: () => [] },
        aria2TaskService: {
            getTaskStatus: (gid, cb) => request('task', gid, cb),
            getBtTaskPeers: (task, cb) => request('peers', task.gid, cb),
            getTaskStatusAndBtPeers: (gid, cb) => request('combined', gid, cb),
            estimateHealthPercentFromPeers: () => 50
        }
    };
    vm.runInNewContext(source, {
        $: () => ({ on: noop }),
        angular: { element: () => ({ on: noop }),
            module: () => ({ controller: (name, args) => { controller = args; } }) }
    });
    controller[controller.length - 1](...controller.slice(0, -1).map(key => dependencies[key]));
    return { pending, root, scope, tick: () => tick(), destroy: () => destroy(),
        cancelled: () => cancelled };
}

const flush = () => new Promise(resolve => setImmediate(resolve));
const task = { gid: 'synthetic', status: 'active', bittorrent: {} };

function checkMalformedResponses() {
    let service;
    let reply;
    const dependencies = {
        aria2RpcService: {
            tellStatus: (context, contextOnly) => contextOnly ? context : context.callback(reply),
            getPeers: (context, contextOnly) => contextOnly ? context : context.callback(reply),
            multicall: context => context.callback(reply)
        },
        ariaNgLocalizationService: { getLocalizedText: value => value },
        ariaNgLogService: { debug: noop, warn: noop }, aria2Errors: {}
    };
    vm.runInNewContext(fs.readFileSync('src/scripts/services/aria2TaskService.js', 'utf8'), {
        angular: { isArray: Array.isArray, module: () => ({ factory: (name, args) => {
            service = args[args.length - 1](...args.slice(0, -1).map(key => dependencies[key]));
        } }) }
    });
    for (const data of [undefined, null, [], {}, [[null]], [{ code: 1, message: 'synthetic fault' }],
        [[{ gid: 'other', status: 'active' }]], [[{ gid: 'synthetic' }]]]) {
        reply = { success: true, data: structuredClone(data) };
        service.getTaskStatusAndBtPeers('synthetic', response => assert.equal(response.success, false));
    }
    for (const data of [undefined, null, {}, { gid: 'other', status: 'active' }, { gid: 'synthetic' }]) {
        reply = { success: true, data };
        service.getTaskStatus('synthetic', response => assert.equal(response.success, false));
    }
    for (const data of [undefined, null, {}, 'invalid']) {
        reply = { success: true, data };
        service.getBtTaskPeers(task, response => assert.equal(response.success, false));
    }
    reply = { success: true, data: [[{ ...task, totalLength: '100', completedLength: '25' }],
        { code: 1, message: 'synthetic peer fault' }] };
    service.getTaskStatusAndBtPeers('synthetic', response => {
        assert.equal(response.success, true, 'Peer faults must not discard valid task data');
        assert.equal(response.task.gid, 'synthetic');
        assert.equal(response.peers, undefined);
    });
}

(async () => {
    checkMalformedResponses();
    const original = fixture(execFileSync('git', ['show', 'HEAD:' + file], { encoding: 'utf8' }));
    const source = fs.readFileSync(file, 'utf8');
    const current = fixture(source);
    for (let i = 0; i < 20; i++) { original.tick(); current.tick(); }
    assert.equal(current.pending.length, 1, 'Slow detail polls must not overlap');
    console.log(JSON.stringify({ timerTicks: 20, baselineRequests: original.pending.length,
        currentRequests: current.pending.length }));

    const first = current.pending[0];
    first.callback({ success: true, data: task });
    first.resolve();
    await flush();
    assert.equal(current.pending[1].kind, 'peers');
    for (let i = 0; i < 20; i++) { current.tick(); }
    assert.equal(current.pending.length, 2, 'Initial peer fetch must hold the cycle guard');
    current.pending[1].callback({ success: true, data: [] });
    current.pending[1].resolve();
    await flush();
    current.tick();
    assert.equal(current.pending[2].kind, 'combined');
    current.pending[2].callback({ success: false, data: { message: 'temporary failure' } });
    current.pending[2].resolve();
    await flush();
    assert.equal(current.cancelled(), false, 'Temporary failures must not stop polling');
    current.tick();
    assert.equal(current.pending.length, 4);
    current.pending[3].reject(new Error('synthetic timeout'));
    await flush();
    current.tick();
    assert.equal(current.pending.length, 5, 'Rejected transport must release the guard');
    current.destroy();
    current.pending[4].callback({ success: true, task: { ...task, gid: 'late' }, peers: [] });
    current.pending[4].resolve();
    await flush();
    current.tick();
    assert.equal(current.pending.length, 5);
    assert.equal(current.scope.task.gid, 'synthetic', 'Destroyed route must ignore late task data');
    assert.equal(current.cancelled(), true);

    const empty = fixture(source);
    empty.pending[0].resolve();
    await flush();
    empty.tick();
    assert.equal(empty.pending.length, 2, 'Empty transport reply must release the guard');
    empty.pending[1].callback({ success: false, data: { message: 'Unauthorized' } });
    empty.pending[1].resolve();
    await flush();
    assert.equal(empty.cancelled(), true, 'Authorization failures must stop polling');

    const peerAuth = fixture(source);
    peerAuth.pending[0].callback({ success: true, data: task });
    peerAuth.pending[0].resolve();
    peerAuth.pending[1].callback({ success: false, data: { message: 'Unauthorized' } });
    peerAuth.pending[1].resolve();
    await flush();
    assert.equal(peerAuth.cancelled(), true, 'Peer authorization failures must stop polling');

    const editing = fixture(source);
    editing.scope.showChooseFilesToolbar();
    editing.pending[0].callback({ success: true, data: task });
    editing.pending[0].resolve();
    await flush();
    assert.equal(editing.scope.task, undefined, 'Late refresh must not overwrite file selection');
    editing.scope.cancelChooseFiles();
    assert.equal(editing.pending.length, 2);
    editing.destroy();
    editing.pending[1].callback({ success: true, data: task });
    editing.pending[1].resolve();
    await flush();
    assert.equal(editing.pending.length, 2, 'Destroyed route must not start a peer request');
    console.log('Detail/peer bounds, transient failure, timeout, empty reply, authorization, editing and route cleanup passed.');
})().catch(error => { console.error(error); process.exitCode = 1; });
