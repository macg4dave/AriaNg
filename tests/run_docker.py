"""Set up disposable Docker RPC credentials, build, and run integration tests."""

import argparse
import os
from pathlib import Path
import secrets
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]
SECRET = ROOT / ".local" / "aria2" / "rpc-secret"


def run(*args):
    subprocess.run(args, cwd=ROOT, check=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--setup-only", action="store_true")
    parser.add_argument("--browser", action="store_true")
    parser.add_argument("--channel", default="msedge")
    args = parser.parse_args()
    SECRET.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(SECRET.parent, 0o700)
    if not SECRET.exists():
        with SECRET.open("x", encoding="ascii") as file:
            os.chmod(SECRET, 0o600)
            file.write(secrets.token_hex(32))
    token = SECRET.read_text(encoding="ascii").strip()
    if len(token) != 64 or any(c not in "0123456789abcdef" for c in token):
        raise SystemExit("Invalid local test token; remove .local/aria2/rpc-secret and retry")
    # Compose file secrets are bind mounts on Linux: allow www-data to read the
    # file while its private parent directory prevents other host users traversing.
    os.chmod(SECRET, 0o444)
    if args.setup_only:
        print("Local test credentials ready; no secret values printed")
        sys.exit(0)
    run("docker", "compose", "up", "--build", "-d", "--wait", "--wait-timeout", "120")
    try:
        smoke = [sys.executable, "tests/docker_smoke.py"]
        if args.browser:
            smoke += ["--browser", "--channel", args.channel]
        run(*smoke)
        command = [sys.executable, "tests/aria2_integration.py"]
        if args.browser:
            command += ["--browser", "--channel", args.channel]
        run(*command)
    except subprocess.CalledProcessError:
        raise SystemExit("Docker integration tests failed (test containers left for inspection)")
