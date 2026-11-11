"""Check the local Docker frontend; optionally exercise it in Chromium."""

import argparse
import re
from urllib.error import HTTPError
from urllib.parse import urljoin, urlparse
from urllib.request import urlopen


def http_checks(base):
    with urlopen(base + "/", timeout=10) as response:
        html = response.read().decode("utf-8")
        assert response.status == 200
        assert response.headers["Cache-Control"] == "no-cache"
        assert response.headers["X-Content-Type-Options"] == "nosniff"
    assets = {
        path for path in re.findall(r'(?:src|href)="([^"#]+)"', html)
        if not urlparse(path).scheme and not path.startswith("//")
        and "{{" not in path
    }
    assert assets, "No static assets found"
    for path in sorted(assets) + ["healthz", "langs/fr_FR.txt", "LICENSE"]:
        with urlopen(urljoin(base + "/", path), timeout=10) as response:
            assert response.status == 200, path
            assert response.read(), path
    for path in ["missing.js", ".env", ".git/config", "package.json"]:
        try:
            urlopen(base + "/" + path, timeout=10).close()
        except HTTPError as error:
            assert error.code == 404, path
        else:
            raise AssertionError("Unexpected public path: " + path)
    print(f"PASS HTTP: index, {len(assets)} assets, language, license, health, 404s")


def browser_checks(base, channel):
    from playwright.sync_api import sync_playwright

    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(channel=channel, headless=True)
        context = browser.new_context(viewport={"width": 1440, "height": 900})
        errors = []
        failed_assets = []

        def route_request(route):
            # Only the test web origin is reachable; never contact an RPC server.
            if urlparse(route.request.url).netloc == urlparse(base).netloc:
                route.continue_()
            else:
                route.abort()

        context.route("**/*", route_request)
        # Block WebSocket RPC too, independently of HTTP interception.
        context.add_init_script("window.WebSocket = class { constructor() { "
                                "throw new Error('RPC disabled by smoke test'); } };")
        page = context.new_page()
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.on("response", lambda response: failed_assets.append(response.url)
                if response.status >= 400 and
                urlparse(response.url).netloc == urlparse(base).netloc else None)
        for route in ["downloading", "waiting", "stopped", "new", "settings/ariang", "status"]:
            page.goto(base + "/#!/" + route, wait_until="networkidle")
            page.wait_for_function("window.angular && "
                                   "angular.element(document.body).injector()")
            assert page.evaluate("angular.element(document.body).injector()"
                                 ".get('$location').path()") == "/" + route, route
            assert page.locator(".content-wrapper").inner_text().strip(), route

        page.goto(base + "/#!/settings/ariang", wait_until="networkidle")
        theme = page.locator('select[ng-model="context.settings.theme"]')
        theme.select_option("dark")
        page.wait_for_function("document.body.classList.contains('theme-dark')")
        page.reload(wait_until="networkidle")
        assert theme.input_value() == "dark", "Theme not persisted"
        assert "theme-dark" in page.locator("body").get_attribute("class")
        theme.select_option("light")
        page.wait_for_function("!document.body.classList.contains('theme-dark')")
        language = page.locator('select[ng-model="context.settings.language"]')
        # Angular ng-options prefixes string values in the DOM.
        language.select_option("string:fr_FR")
        page.reload(wait_until="networkidle")
        assert language.input_value() == "string:fr_FR", "Language not persisted"
        assert "Langage" in page.locator(".content-wrapper").inner_text()
        language.select_option("string:en")

        page.set_viewport_size({"width": 390, "height": 844})
        page.goto(base + "/#!/downloading", wait_until="networkidle")
        assert page.locator(".content-wrapper").is_visible()
        assert page.evaluate("document.documentElement.scrollWidth <= window.innerWidth"), \
            "Mobile viewport overflows horizontally"
        assert not errors, "Uncaught browser errors: " + repr(errors)
        assert not failed_assets, "Failed local assets: " + repr(failed_assets)
        context.close()
        browser.close()
    print("PASS browser: routes, themes, language persistence, mobile layout, runtime errors")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base-url", default="http://127.0.0.1:8080")
    parser.add_argument("--browser", action="store_true")
    parser.add_argument("--channel", default="msedge")
    args = parser.parse_args()
    base = args.base_url.rstrip("/")
    assert urlparse(base).hostname in ("127.0.0.1", "localhost", "::1"), \
        "Smoke tests must target a local disposable environment"
    http_checks(base)
    if args.browser:
        browser_checks(base, args.channel)
