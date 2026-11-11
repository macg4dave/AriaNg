'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
let factory;
vm.runInNewContext(fs.readFileSync('src/scripts/services/aria2HttpRpcService.js', 'utf8'), {
    angular: { module: () => ({ factory: (name, args) => { factory = args; } }),
        isString: value => typeof value === 'string', isObject: value => value !== null && typeof value === 'object',
        isArray: Array.isArray, isUndefined: value => value === undefined, toJson: JSON.stringify }
});

async function run({ gateway = false, url = 'http://fixture.invalid/rpc', result, failure, throws = false }) {
    let requests = 0;
    let successes = 0;
    let errors = 0;
    let connected = 0;
    let disconnected = 0;
    let request;
    const dependencies = {
        $http: options => { requests++; request = options; return failure ? Promise.reject(failure) : Promise.resolve(result); },
        $q: { when: () => Promise.resolve() }, $window: { URL, location: { origin: 'http://fixture.invalid' } },
        ariaNgConstants: { httpRequestTimeout: 20000 }, ariaNgCommonService: {}, ariaNgLogService: {},
        ariaNgSettingService: { getCurrentRpcUrl: () => url, getCurrentRpcHttpMethod: () => 'GET',
            getCurrentRpcRequestHeaders: () => '', isCurrentRpcUseGateway: () => gateway,
            getCurrentRpcSecret: () => 'synthetic-token' }
    };
    const service = factory[factory.length - 1](...factory.slice(0, -1).map(key => dependencies[key]));
    const pending = service.request({ requestBody: { jsonrpc: '2.0', id: 1, method: 'aria2.getVersion' },
        successCallback: () => { successes++; if (throws) { throw new Error('callback fixture'); } },
        errorCallback: (id, error) => { errors++; assert.equal(typeof error.message, 'string'); },
        connectionSuccessCallback: () => connected++, connectionFailedCallback: () => disconnected++ });
    if (throws) { await assert.rejects(pending, /callback fixture/); } else { await pending; }
    return { requests, successes, errors, connected, disconnected, request };
}

(async () => {
    for (const response of [undefined, '', '<html>upstream unavailable</html>', { id: 1 }, { id: 1, error: { message: 'RPC failure', code: -1 } }]) {
        const value = await run({ result: { data: response, status: 200 } });
        assert.equal(value.errors, 1);
        assert.equal(value.successes, 0);
    }
    for (const failure of [{ status: -1 }, { status: 502, data: 'bad gateway' }, { status: 401, data: { id: 1, error: { message: 'Unauthorized' } } }]) {
        const value = await run({ failure });
        assert.equal(value.errors, 1);
        assert.equal(value.disconnected, 1);
    }
    const value = await run({ gateway: true, result: { data: { id: 1, result: {} }, status: 200 } });
    assert.equal(value.successes, 1);
    assert.equal(value.connected, 1);
    assert.equal(value.request.method, 'POST');
    assert.equal(value.request.headers.Authorization, 'Bearer synthetic-token');
    assert.ok(!value.request.data.includes('synthetic-token'));
    for (const url of ['https://other.invalid/rpc', 'ws://fixture.invalid/rpc', 'http://[invalid']) {
        const rejected = await run({ gateway: true, url });
        assert.equal(rejected.requests, 0);
        assert.equal(rejected.errors, 1);
    }
    const callback = await run({ result: { data: { id: 1, result: true }, status: 200 }, throws: true });
    assert.equal(callback.errors, 0, 'application callback failures are not transport failures');
    console.log('PASS HTTP RPC: JSON-RPC errors in HTTP 200, malformed replies, transport/auth failures, callback settlement and same-origin gateway credentials.');
})().catch(error => { console.error('HTTP RPC check failed: ' + error.name); process.exitCode = 1; });
