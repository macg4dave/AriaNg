'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
let service;
let onMessage;
let onClose;
let throwOnSend = false;
let reconnects = 0;
const timers = new Set();
const timeout = handler => { timers.add(handler); return handler; };
timeout.cancel = handler => timers.delete(handler);
const socket = {
    readyState: 1,
    onMessage: handler => { onMessage = handler; },
    onClose: handler => { onClose = handler; },
    onOpen: () => {},
    send: () => { if (throwOnSend) { throw new Error('synthetic send failure'); } },
    reconnect: () => { reconnects++; }
};
const noop = () => {};
const dependencies = {
    $q: { defer: () => ({ promise: {}, resolve: noop, reject: noop }) },
    $websocket: () => socket,
    $timeout: timeout,
    ariaNgConstants: { httpRequestTimeout: 20000 },
    ariaNgSettingService: {
        getCurrentRpcUrl: () => 'ws://example.invalid/jsonrpc',
        getWebSocketReconnectInterval: () => 0
    },
    ariaNgLogService: { debug: noop, warn: noop }
};
vm.runInNewContext(fs.readFileSync('src/scripts/services/aria2WebSocketRpcService.js', 'utf8'), {
    angular: {
        fromJson: JSON.parse, toJson: JSON.stringify, isArray: Array.isArray,
        module: () => ({ factory: (name, args) => {
            service = args[args.length - 1](...args.slice(0, -1).map(key => dependencies[key]));
        } })
    }
});
let successes = 0;
let failures = 0;
function request(id) {
    service.request({ uniqueId: id, requestBody: { id, method: 'aria2.tellActive' },
        successCallback: () => { successes++; },
        errorCallback: received => { assert.equal(received, id); failures++; }
    });
}
request('timeout');
assert.equal(timers.size, 1);
[...timers][0]();
assert.equal(failures, 1);
assert.equal(timers.size, 0);
onMessage({ data: JSON.stringify({ id: 'timeout', result: [] }) });
assert.equal(successes, 0, 'late timeout reply must be ignored');
request('first');
request('second');
onClose({});
assert.equal(failures, 3, 'close must fail all requests even without auto reconnect');
assert.equal(timers.size, 0);
request('recovered');
onMessage({ data: JSON.stringify({ id: 'recovered', result: [] }) });
assert.equal(successes, 1);
assert.equal(timers.size, 0);
request('reconnect');
service.reconnect({});
assert.equal(reconnects, 1);
assert.equal(failures, 4);
throwOnSend = true;
request('send-failure');
assert.equal(failures, 5);
assert.equal(timers.size, 0);
throwOnSend = false;
socket.readyState = 0;
request('connecting');
assert.equal(failures, 6, 'disconnected operations must fail instead of being queued for replay');
assert.equal(timers.size, 0);
console.log('WebSocket timeout, late reply, close, reconnect, send failure, and recovery checks passed.');
