# Development

## Source map

- `src/index.html`: application shell and asset references.
- `src/scripts/`: AngularJS config, controllers, services, directives, and filters.
- `src/views/`: HTML templates; `src/styles/`: styles and themes.
- `src/langs/` and `i18n/`: translations and reference material.
- `gulpfile.js`: development and build pipelines.
- `.github/workflows/`: inherited build/release automation; review destinations
  and permissions before adapting it to this fork.
- `.tmp/` and `dist/`: generated output; edit source instead.

## Current commands

| Purpose | Command |
| --- | --- |
| Install locked dependencies | `npm ci` |
| Standard build | `npm run build` |
| All-in-one build | `npx gulp clean build-bundle` |
| Development server | `npx gulp serve` |
| Preview standard build over HTTP | `npx gulp serve:dist` |
| Build/start Debian/Nginx/aria2 and run HTTP/RPC tests | `python tests/run_docker.py` |
| Complete Docker integration suite with Edge | `python tests/run_docker.py --browser --channel msedge` |
| HTTP smoke checks against local container | `python tests/docker_smoke.py` |
| UI smoke checks with Playwright and Edge installed | `python tests/docker_smoke.py --browser --channel msedge` |

See [Docker testing](DOCKER.md) for container configuration and shutdown commands.

The npm lockfile has been converted from format 1 to format 3 for modern `npm ci`.
Existing locked package versions and the dropdown Git commit are retained, with
missing optional dependency metadata filled in on Linux. Builds use public
dependencies and do not receive SSH credentials.

`npm test` currently succeeds without testing anything. The build invokes the
existing Gulp lint task; neither establishes comprehensive behavioral correctness.
Use meaningful checks for the affected workflow. For UI changes, verify responsive
layouts, themes, translations where affected, and console errors when a browser
is available. Never use production RPC services for routine testing.

## Future server components

No Go module, Cargo workspace, or server toolchain is selected yet. Document the
chosen version, supported platform, source layout, and exact commands here when
introducing one. Follow language-native formatting (gofmt or rustfmt); JavaScript
formatting conventions do not override them. Maintain generated dependency locks.

For Go, relevant checks include formatting, `go vet ./...`, `go test ./...`, the
race detector where supported, and a build for the intended deployment target.
For Rust, include formatting, Clippy, tests, and locked release builds for supported
features/targets. Record actual module/workspace commands once they exist; these
are verification expectations, not commands that work in the current repository.
