(function () {
    'use strict';

    angular.module('ariaNg').factory('aria2HttpRpcService', ['$http', '$q', '$window', 'ariaNgConstants', 'ariaNgCommonService', 'ariaNgSettingService', function ($http, $q, $window, ariaNgConstants, ariaNgCommonService, ariaNgSettingService) {
        var rpcUrl = ariaNgSettingService.getCurrentRpcUrl();
        var method = ariaNgSettingService.getCurrentRpcHttpMethod();
        var requestHeaders = ariaNgSettingService.getCurrentRpcRequestHeaders();
        var gatewayMode = ariaNgSettingService.isCurrentRpcUseGateway();
        var gatewayToken = gatewayMode ? ariaNgSettingService.getCurrentRpcSecret() : '';
        if (gatewayMode) {
            method = 'POST';
        }

        var getUrlWithQueryString = function (url, parameters) {
            if (!url || url.length < 1) {
                return url;
            }

            var queryString = '';

            for (var key in parameters) {
                if (!parameters.hasOwnProperty(key)) {
                    continue;
                }

                var value = parameters[key];

                if (value === null || angular.isUndefined(value)) {
                    continue;
                }

                if (queryString.length > 0) {
                    queryString += '&';
                }

                if (angular.isObject(value) || angular.isArray(value)) {
                    value = angular.toJson(value);
                    value = ariaNgCommonService.base64Encode(value);
                    value = encodeURIComponent(value);
                }

                queryString += key + '=' + value;
            }

            if (queryString.length < 1) {
                return url;
            }

            if (url.indexOf('?') < 0) {
                queryString = '?' + queryString;
            } else {
                queryString = '&' + queryString;
            }

            return url + queryString;
        };

        return {
            request: function (context) {
                if (!context) {
                    return;
                }

                var requestContext = {
                    url: rpcUrl,
                    method: method,
                    headers: {},
                    timeout: ariaNgConstants.httpRequestTimeout
                };

                if (requestContext.method === 'POST') {
                    requestContext.data = angular.toJson(context.requestBody);
                    requestContext.headers['Content-Type'] = 'application/json';
                } else if (requestContext.method === 'GET') {
                    requestContext.url = getUrlWithQueryString(requestContext.url, context.requestBody);
                }

                if (requestHeaders) {
                    var lines = requestHeaders.split('\n');

                    for (var i = 0; i < lines.length; i++) {
                        var items = lines[i].split(':');

                        if (items.length !== 2) {
                            continue;
                        }

                        var name = items[0].trim();
                        var value = items[1].trim();

                        if (!gatewayMode || name.toLowerCase() !== 'authorization') {
                            requestContext.headers[name] = value;
                        }
                    }
                }

                if (gatewayToken) {
                    requestContext.headers.Authorization = 'Bearer ' + gatewayToken;
                }

                var connectionError = function (response) {
                    var data = response && response.data;
                    var error = data && data.error && angular.isString(data.error.message) ? data.error : { message: 'Cannot connect to aria2!' };
                    var callback = data && data.error && response.status >= 200 && response.status < 500 &&
                            response.status !== 401 && response.status !== 403 ?
                        context.connectionSuccessCallback : context.connectionFailedCallback;
                    if (callback) {
                        callback({ rpcUrl: rpcUrl, method: method });
                    }
                    if (context.errorCallback) {
                        context.errorCallback(data && data.id, error);
                    }
                };

                // Gateway credentials may only go to the configured same-origin proxy.
                var validGateway = false;
                if (gatewayMode) {
                    try {
                        var gatewayUrl = new $window.URL(rpcUrl);
                        validGateway = gatewayUrl.origin === $window.location.origin &&
                            (gatewayUrl.protocol === 'http:' || gatewayUrl.protocol === 'https:');
                    } catch (error) {
                        validGateway = false;
                    }
                }
                if (gatewayMode && !validGateway) {
                    connectionError({ data: { error: { message: 'Gateway requires a same-origin HTTP connection.' } } });
                    return $q.when();
                }

                return $http(requestContext).then(function onSuccess(response) {
                    var data = response.data;

                    if (!data || data.error || !Object.prototype.hasOwnProperty.call(data, 'result')) {
                        connectionError(response);
                        return;
                    }

                    if (context.connectionSuccessCallback) {
                        context.connectionSuccessCallback({
                            rpcUrl: rpcUrl,
                            method: method
                        });
                    }

                    if (context.successCallback) {
                        context.successCallback(data.id, data.result);
                    }
                }, connectionError);
            },
            reconnect: function () {
                //Not implement
            },
            on: function (eventName, callback) {
                //Not implement
            }
        };
    }]);
}());
