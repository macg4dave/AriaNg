"""Exercise real aria2 in the disposable local Compose environment."""

import argparse
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import time
from urllib.error import HTTPError
from urllib.request import Request, urlopen
import uuid


ROOT = Path(__file__).resolve().parents[1]
BASE = "http://127.0.0.1:8080"
FIXTURE = "http://web:8080/test-fixture.bin"


def rpc(method, *params, token=None):
    values = (["token:" + token] if token else []) + list(params)
    request = Request(BASE + "/jsonrpc", json.dumps({
        "jsonrpc": "2.0", "id": "test", "method": method, "params": values,
    }).encode(), {"Content-Type": "application/json"})
    try:
        with urlopen(request, timeout=10) as response:
            result = json.load(response)
    except HTTPError as error:
        result = json.load(error)
    if "error" in result:
        # Do not include full requests or backend messages that might expose secrets.
        raise RuntimeError("RPC rejected " + method)
    return result["result"]


def wait_status(gid, wanted, token):
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        status = rpc("aria2.tellStatus", gid, token=token)
        if status["status"] == wanted:
            return status
        if status["status"] == "error" and wanted != "error":
            raise AssertionError("Unexpected download failure")
        time.sleep(0.1)
    raise AssertionError("Task did not reach " + wanted)


def integration(token):
    for wrong in (None, "invalid-test-token"):
        try:
            rpc("aria2.getVersion", token=wrong)
        except RuntimeError:
            pass
        else:
            raise AssertionError("Unauthenticated RPC accepted")
    assert rpc("aria2.getVersion", token=token)["version"]
    assert "numActive" in rpc("aria2.getGlobalStat", token=token)
    gids = []
    try:
        name = "rpc-test-" + uuid.uuid4().hex + ".bin"
        gid = rpc("aria2.addUri", [FIXTURE], {
            "out": name, "pause": "true", "max-download-limit": "16K",
            "checksum": "sha-256=" + hashlib.sha256(bytes(65536)).hexdigest(),
        }, token=token)
        gids.append(gid)
        wait_status(gid, "paused", token)
        assert any(t["gid"] == gid for t in rpc("aria2.tellWaiting", 0, 100, token=token))
        rpc("aria2.unpause", gid, token=token)
        wait_status(gid, "active", token)
        rpc("aria2.forcePause", gid, token=token)
        wait_status(gid, "paused", token)
        rpc("aria2.unpause", gid, token=token)
        status = wait_status(gid, "complete", token)
        assert status["completedLength"] == status["totalLength"] == "65536"
        content = subprocess.check_output([
            "docker", "compose", "exec", "-T", "aria2", "cat", "/downloads/" + name,
        ], cwd=ROOT)
        assert content == bytes(65536), "Downloaded bytes differ from fixture"
        rpc("aria2.removeDownloadResult", gid, token=token)
        gids.remove(gid)

        gid = rpc("aria2.addUri", [FIXTURE], {"pause": "true"}, token=token)
        gids.append(gid)
        rpc("aria2.remove", gid, token=token)
        assert not any(t["gid"] == gid for t in
                       rpc("aria2.tellWaiting", 0, 100, token=token)
                       + rpc("aria2.tellActive", token=token))
        # aria2 may discard a never-started task without a stopped result.
        try:
            rpc("aria2.removeDownloadResult", gid, token=token)
        except RuntimeError:
            pass
        gids.remove(gid)

        gid = rpc("aria2.addUri", ["http://web:8080/missing-test-file"], token=token)
        gids.append(gid)
        assert wait_status(gid, "error", token)["errorCode"] != "0"
        rpc("aria2.removeDownloadResult", gid, token=token)
        gids.remove(gid)
    finally:
        for gid in gids:
            for method in ("aria2.forceRemove", "aria2.removeDownloadResult"):
                try:
                    rpc(method, gid, token=token)
                except RuntimeError:
                    pass
    print("PASS aria2: auth rejection, version/stats, queued/active/pause/resume, "
          "download/checksum/bytes, removal, HTTP failure")


def browser_test(token, channel):
    from playwright.sync_api import sync_playwright

    name = "ui-test-" + uuid.uuid4().hex + ".bin"
    gid = None
    with sync_playwright() as p:
        browser = p.chromium.launch(channel=channel, headless=True)
        context = browser.new_context()
        settings = {
            "rpcHost": "127.0.0.1", "rpcPort": "8080", "rpcInterface": "jsonrpc",
            "protocol": "http", "httpMethod": "POST", "language": "en",
            "secret": base64.b64encode(token.encode()).decode(),
            "extendRpcServers": [], "globalStatRefreshInterval": 1000,
            "downloadTaskRefreshInterval": 1000,
        }
        context.add_init_script("localStorage.setItem('AriaNg.Options', "
                                + json.dumps(json.dumps(settings)) + ");")
        context.route("**/*", lambda route: route.continue_()
                      if route.request.url.startswith(BASE + "/") else route.abort())
        page = context.new_page()
        errors = []
        page.on("pageerror", lambda error: errors.append(str(error)))
        try:
            page.goto(BASE + "/#!/new", wait_until="networkidle")
            version = page.evaluate("""token => new Promise((resolve, reject) => {
                const socket = new WebSocket('ws://127.0.0.1:8080/jsonrpc');
                const timer = setTimeout(() => { socket.close(); reject(new Error('WS timeout')); }, 5000);
                socket.onopen = () => socket.send(JSON.stringify({jsonrpc: '2.0',
                    id: 'ws-test', method: 'aria2.getVersion', params: ['token:' + token]}));
                socket.onerror = () => { clearTimeout(timer); reject(new Error('WS failure')); };
                socket.onmessage = event => {
                    clearTimeout(timer); socket.close();
                    const result = JSON.parse(event.data);
                    if (result.result) resolve(result.result.version);
                    else reject(new Error('WS RPC rejected'));
                };
            })""", token)
            assert version, "WebSocket RPC returned no version"
            page.locator('textarea[name="urls"]').fill(FIXTURE + "?" + name)
            page.locator('form[name="newTaskForm"] button[type="submit"]').click()
            deadline = time.monotonic() + 15
            while time.monotonic() < deadline:
                tasks = rpc("aria2.tellStopped", 0, 100, token=token)
                tasks += rpc("aria2.tellActive", token=token)
                task = next((t for t in tasks if any(
                    name in u["uri"] for f in t["files"] for u in f["uris"])), None)
                if task:
                    gid = task["gid"]
                    break
                time.sleep(0.1)
            assert gid, "UI did not submit download to aria2"
            wait_status(gid, "complete", token)
            page.goto(BASE + "/#!/stopped", wait_until="networkidle")
            page.locator('[data-gid="' + gid + '"]').wait_for(state="visible")
            assert not errors, "Uncaught browser error"
        finally:
            if gid:
                rpc("aria2.removeDownloadResult", gid, token=token)
            context.close()
            browser.close()
    print("PASS UI + aria2: authenticated HTTP/WebSocket RPC, submit download, completed task shown")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--browser", action="store_true")
    parser.add_argument("--channel", default="msedge")
    args = parser.parse_args()
    token = (ROOT / ".local/aria2/rpc-secret").read_text(encoding="ascii").strip()
    integration(token)
    if args.browser:
        browser_test(token, args.channel)
