'use strict';

// Frontend behavior and CPU microbenchmark. No network or private fixtures.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const { execFileSync } = require('node:child_process');
const { performance } = require('node:perf_hooks');
const file = 'src/scripts/services/aria2TaskService.js';
const common = {
    countArray: (items, value) => items.filter(item => item === value).length
};

function load(source) {
    let service;
    const noop = () => {};
    const dependencies = {
        ariaNgCommonService: common,
        ariaNgLocalizationService: { getLocalizedText: value => value },
        ariaNgLogService: { debug: noop, warn: noop },
        aria2Errors: {},
        ariaNgConstants: { defaultPathSeparator: '/' }
    };
    vm.runInNewContext(source, {
        angular: {
            isArray: Array.isArray,
            isDefined: value => value !== undefined,
            module: () => ({ factory: (name, args) => {
                service = args[args.length - 1](...args.slice(0, -1).map(key => dependencies[key]));
            } })
        }
    });
    return service;
}

function task(fileCount = 1) {
    return {
        gid: 'synthetic', status: 'active', totalLength: '10000',
        completedLength: '2500', uploadLength: '500', downloadSpeed: '100',
        uploadSpeed: '10', numPieces: '7', pieceLength: '1024', bitfield: 'ff',
        files: Array.from({ length: fileCount }, (_, index) => ({
            index: String(index + 1), path: '/synthetic/file-' + index,
            length: '10000', completedLength: '2500', selected: 'true',
            uris: [{ uri: 'https://example.invalid/fixture' }]
        }))
    };
}

const current = load(fs.readFileSync(file, 'utf8'));
// Compare with a supplied committed revision; default is the pre-change checkout.
const revision = process.argv[2] || 'HEAD';
const baseline = load(execFileSync('git', ['show', revision + ':' + file], { encoding: 'utf8' }));

for (let pieces = 0; pieces <= 17; pieces++) {
    for (const bitfield of ['', '00', 'ff', 'a5f8', 'FFFF', 'g1']) {
        const item = task();
        item.numPieces = String(pieces);
        item.bitfield = bitfield;
        const expected = pieces > 0 ? common.countArray(current.getPieceStatus(bitfield, pieces), true) : 0;
        current.processDownloadTasks([item]);
        assert.equal(item.completedPieces, expected, bitfield + '/' + pieces);
    }
}

const cached = task(3);
current.processDownloadTasks([cached]);
const initial = JSON.parse(JSON.stringify(cached));
cached.downloadSpeed = '200';
current.processDownloadTasks([cached], false, true);
assert.equal(cached.remainTime, 37.5);
assert.equal(cached.selectedFileCount, 3);
assert.deepEqual(cached.files, initial.files);
// A fresh/full response must recalculate file selection and progress.
cached.files[0].selected = 'false';
cached.files[0].completedLength = '5000';
current.processDownloadTasks([cached]);
assert.equal(cached.selectedFileCount, 2);
assert.equal(cached.files[0].completePercent, 50);
const fresh = task(2);
current.processDownloadTasks([fresh], false, true);
assert.equal(fresh.selectedFileCount, 2);

function measure(service, tasks, reuse) {
    for (let i = 0; i < 5; i++) {
        service.processDownloadTasks(tasks, false, reuse);
    }
    const samples = [];
    for (let i = 0; i < 30; i++) {
        const start = performance.now();
        service.processDownloadTasks(tasks, false, reuse);
        samples.push(performance.now() - start);
    }
    samples.sort((a, b) => a - b);
    return { medianMs: +samples[15].toFixed(3), p95Ms: +samples[28].toFixed(3) };
}

for (const [taskCount, fileCount] of [[1000, 1], [1000, 100]]) {
    const original = Array.from({ length: taskCount }, () => task(fileCount));
    // List RPC responses do not contain piece fields.
    original.forEach(item => {
        delete item.bitfield;
        delete item.numPieces;
        delete item.pieceLength;
    });
    const updated = structuredClone(original);
    baseline.processDownloadTasks(original);
    current.processDownloadTasks(updated);
    assert.deepEqual(JSON.parse(JSON.stringify(updated)), JSON.parse(JSON.stringify(original)));
    console.log(JSON.stringify({ taskCount, fileCount, samples: 30,
        baseline: measure(baseline, original, true),
        current: measure(current, updated, true) }));
}

// Simulate timer ticks, slow RPC, failures, recovery, and leaving the route.
let tick;
let destroyed;
let callback;
let settle;
let requests = 0;
let cancelled = false;
let controller;
const interval = handler => { tick = handler; return 1; };
interval.cancel = () => { cancelled = true; };
const root = { taskContext: { list: [] }, keydownActions: {} };
const scope = { $on: (event, handler) => {
    if (event === '$destroy') { destroyed = handler; }
} };
const dependencies = {
    $rootScope: root, $scope: scope, $location: { path: () => '/downloading' },
    $interval: interval, dragulaService: { options: () => {} },
    aria2RpcErrors: { Unauthorized: { message: 'Unauthorized' } },
    ariaNgSettingService: { getDownloadTaskRefreshInterval: () => 1000, isCurrentRpcUseGateway: () => false },
    ariaNgCommonService: {},
    aria2TaskService: { getTaskList: (location, full, handler) => {
        requests++;
        callback = handler;
        return { then: (resolve, reject) => { settle = reject; } };
    } }
};
vm.runInNewContext(fs.readFileSync('src/scripts/controllers/list.js', 'utf8'), {
    angular: { module: () => ({ controller: (name, args) => { controller = args; } }) }
});
controller[controller.length - 1](...controller.slice(0, -1).map(key => dependencies[key]));
for (let i = 0; i < 20; i++) { tick(); }
assert.equal(requests, 1, 'slow request must not accumulate polls');
callback({ success: false, data: { message: 'temporary failure' } });
const previousSettlement = settle;
tick();
assert.equal(requests, 2, 'polling must recover after failure');
previousSettlement();
tick();
assert.equal(requests, 2, 'settlement of an old request must not release a newer request');
settle();
tick();
assert.equal(requests, 3, 'transport settlement without a callback must release the guard');
callback({ success: true, context: { requestWholeInfo: true }, data: [] });
tick();
assert.equal(requests, 4);
callback({ success: false, data: { message: 'Unauthorized' } });
assert.equal(cancelled, true);
destroyed();
callback({ success: true, context: { requestWholeInfo: true }, data: [{ gid: 'late' }] });
assert.equal(root.taskContext.list.length, 0, 'late response must not update another route');
console.log('Task processing and polling regression checks passed.');
