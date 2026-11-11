(function () {
    'use strict';

    angular.module('ariaNg').controller('DownloadListController', ['$rootScope', '$scope', '$window', '$location', '$route', '$interval', 'dragulaService', 'aria2RpcErrors', 'ariaNgCommonService', 'ariaNgSettingService', 'aria2TaskService', function ($rootScope, $scope, $window, $location, $route, $interval, dragulaService, aria2RpcErrors, ariaNgCommonService, ariaNgSettingService, aria2TaskService) {
        var location = $location.path().substring(1);
        var downloadTaskRefreshPromise = null;
        var pauseDownloadTaskRefresh = false;
        var needRequestWholeInfo = true;
        var requestInFlight = false;
        var queryGeneration = 0;
        var refreshPending = false;
        // Retain this controller's existing $scope/template contract.
        // eslint-disable-next-line angular/controller-as
        $scope.gatewayEnabled = ariaNgSettingService.isCurrentRpcUseGateway();
        // eslint-disable-next-line angular/controller-as
        $scope.pageState = { offset: 0, limit: 100, total: 0, pages: 1, snapshot: null, loading: false };

        var refreshGatewayPage = function (silent) {
            var generation = queryGeneration;
            var order = ariaNgCommonService.parseOrderType(ariaNgSettingService.getDisplayOrder(location));
            var query = {
                source: location === 'downloading' ? 'active' : location,
                search: $rootScope.searchContext.text || '',
                sort: order.type === 'default' ? 'position' : order.type,
                reverse: !!order.reverse,
                offset: $scope.pageState.offset,
                limit: $scope.pageState.limit
            };
            if ($scope.pageState.snapshot) {
                query.snapshot = $scope.pageState.snapshot;
                $scope.pageState.snapshot = null;
            }
            requestInFlight = true;
            $scope.pageState.loading = true;
            var promise = aria2TaskService.getTaskPage(query, function (response) {
                if (pauseDownloadTaskRefresh || generation !== queryGeneration) {
                    return;
                }
                if (!response.success) {
                    if (response.data.code === -32003 && query.snapshot) {
                        refreshPending = true;
                    } else if (response.data.message === aria2RpcErrors.Unauthorized.message) {
                        $interval.cancel(downloadTaskRefreshPromise);
                    }
                    return;
                }
                var page = response.data;
                $scope.pageState.total = page.total;
                $scope.pageState.pages = Math.max(1, Math.ceil(page.total / query.limit));
                $scope.pageState.revision = page.snapshot;
                if (query.offset > 0 && query.offset >= page.total) {
                    $scope.pageState.offset = Math.max(0, Math.ceil(page.total / query.limit) - 1) * query.limit;
                    refreshPending = true;
                    return;
                }
                $rootScope.taskContext.list = page.tasks;
                $rootScope.taskContext.enableSelectAll = page.tasks.length > 0;
            }, silent);
            var release = function () {
                requestInFlight = false;
                $scope.pageState.loading = false;
                if (refreshPending && !pauseDownloadTaskRefresh) {
                    refreshPending = false;
                    refreshDownloadTask(true);
                }
            };
            promise.then(release, release);
            return promise;
        };

        var refreshDownloadTask = function (silent) {
            if (pauseDownloadTaskRefresh || requestInFlight) {
                return;
            }

            if ($scope.gatewayEnabled) {
                return refreshGatewayPage(silent);
            }

            var request = {};
            requestInFlight = request;
            var promise = aria2TaskService.getTaskList(location, needRequestWholeInfo, function (response) {
                requestInFlight = false;
                if (pauseDownloadTaskRefresh) {
                    return;
                }

                if (!response.success) {
                    if (response.data.message === aria2RpcErrors.Unauthorized.message) {
                        $interval.cancel(downloadTaskRefreshPromise);
                    }

                    return;
                }

                var isRequestWholeInfo = response.context.requestWholeInfo;
                var taskList = response.data;

                if (isRequestWholeInfo) {
                    $rootScope.taskContext.list = taskList;
                    needRequestWholeInfo = false;
                } else {
                    if ($rootScope.taskContext.list && $rootScope.taskContext.list.length > 0) {
                        for (var i = 0; i < $rootScope.taskContext.list.length; i++) {
                            var task = $rootScope.taskContext.list[i];
                            delete task.verifiedLength;
                            delete task.verifyIntegrityPending;
                        }
                    }

                    if (ariaNgCommonService.extendArray(taskList, $rootScope.taskContext.list, 'gid')) {
                        needRequestWholeInfo = false;
                    } else {
                        needRequestWholeInfo = true;
                    }
                }

                if ($rootScope.taskContext.list && $rootScope.taskContext.list.length > 0) {
                    aria2TaskService.processDownloadTasks($rootScope.taskContext.list, false, !isRequestWholeInfo);

                    if (!isRequestWholeInfo) {
                        var hasFullStruct = false;

                        for (var i = 0; i < $rootScope.taskContext.list.length; i++) {
                            var task = $rootScope.taskContext.list[i];

                            if (task.hasTaskName || task.files || task.bittorrent) {
                                hasFullStruct = true;
                                break;
                            }
                        }

                        if (!hasFullStruct) {
                            needRequestWholeInfo = true;
                            $rootScope.taskContext.list.length = 0;
                            return;
                        }
                    }
                }

                $rootScope.taskContext.enableSelectAll = $rootScope.taskContext.list && $rootScope.taskContext.list.length > 0;
            }, silent);

            var releaseRequest = function () {
                if (requestInFlight === request) {
                    requestInFlight = false;
                }
            };

            promise.then(releaseRequest, releaseRequest);
            return promise;
        };

        $scope.getOrderType = function () {
            return $scope.gatewayEnabled ? 'default:asc' : ariaNgSettingService.getDisplayOrder(location);
        };

        // eslint-disable-next-line angular/controller-as
        $scope.changePage = function (direction) {
            var offset = $scope.pageState.offset + direction * $scope.pageState.limit;
            if ($scope.pageState.loading || offset < 0 || offset >= $scope.pageState.total) {
                return;
            }
            queryGeneration++;
            $scope.pageState.offset = offset;
            $scope.pageState.snapshot = $scope.pageState.revision;
            $rootScope.taskContext.selected = {};
            $rootScope.taskContext.list = [];
            $rootScope.taskContext.enableSelectAll = false;
            refreshDownloadTask(false);
        };

        if ($scope.gatewayEnabled) {
            $scope.$watchGroup(['searchContext.text', function () {
                return ariaNgSettingService.getDisplayOrder(location);
            }], function (values, previous) {
                if (values[0] === previous[0] && values[1] === previous[1]) {
                    return;
                }
                queryGeneration++;
                $scope.pageState.offset = 0;
                $scope.pageState.snapshot = null;
                $rootScope.taskContext.selected = {};
                $rootScope.taskContext.list = [];
                $rootScope.taskContext.enableSelectAll = false;
                if (requestInFlight) {
                    refreshPending = true;
                } else {
                    refreshDownloadTask(true);
                }
            });
        }

        $scope.isSupportDragTask = function () {
            if (!ariaNgSettingService.getDragAndDropTasks()) {
                return false;
            }

            var displayOrder = ariaNgCommonService.parseOrderType(ariaNgSettingService.getDisplayOrder(location));

            return location === 'waiting' && displayOrder.type === 'default' &&
                (!$scope.gatewayEnabled || (!displayOrder.reverse && !$rootScope.searchContext.text));
        };

        if (ariaNgSettingService.getDownloadTaskRefreshInterval() > 0) {
            downloadTaskRefreshPromise = $interval(function () {
                refreshDownloadTask(true);
            }, ariaNgSettingService.getDownloadTaskRefreshInterval());
        }

        dragulaService.options($scope, 'task-list', {
            revertOnSpill: true,
            moves: function () {
                return $scope.isSupportDragTask();
            }
        });

        $scope.$on('task-list.drop-model', function (el, target, source) {
            var element = angular.element(target);
            var gid = element.attr('data-gid');
            var index = element.index() + ($scope.gatewayEnabled ? $scope.pageState.offset : 0);

            pauseDownloadTaskRefresh = true;

            aria2TaskService.changeTaskPosition(gid, index, function () {
                pauseDownloadTaskRefresh = false;
                if ($scope.gatewayEnabled) {
                    $scope.pageState.snapshot = null;
                    refreshDownloadTask(true);
                }
            }, true);
        });

        $scope.$on('$destroy', function () {
            pauseDownloadTaskRefresh = true;

            if (downloadTaskRefreshPromise) {
                $interval.cancel(downloadTaskRefreshPromise);
            }
        });

        $rootScope.keydownActions.selectAll = function (event) {
            if (event.preventDefault) {
                event.preventDefault();
            }

            $scope.$apply(function () {
                $scope.selectAllTasks();
            });

            return false;
        };

        $rootScope.keydownActions.delete = function (event) {
            if (event.preventDefault) {
                event.preventDefault();
            }

            $scope.$apply(function () {
                $scope.removeTasks();
            });

            return false;
        };

        $rootScope.loadPromise = refreshDownloadTask(false);
    }]);
}());
