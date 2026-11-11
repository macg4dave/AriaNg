(function () {
    'use strict';

    angular.module('ariaNg').constant('aria2RpcConstants', {
        rpcServiceVersion: '2.0',
        rpcServiceName: 'aria2',
        rpcSystemServiceName: 'system',
        rpcTokenPrefix: 'token:'
    }).constant('aria2RpcErrors', {
        'Invalid Request': {
            tipTextKey: 'rpc.error.gateway.invalid'
        },
        'Upstream unavailable': {
            tipTextKey: 'Cannot connect to aria2!'
        },
        'Gateway busy': {
            tipTextKey: 'rpc.error.gateway.busy'
        },
        'RPC request failed': {
            tipTextKey: 'rpc.error.gateway.failed'
        },
        Unauthorized: {
            message: 'Unauthorized',
            tipTextKey: 'rpc.error.unauthorized'
        }
    });
}());
