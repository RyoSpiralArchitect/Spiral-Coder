"""Exercise the actual native search tool through a loopback, key-free server."""
from contextlib import contextmanager
from http.client import HTTPConnection
import json
import os
from pathlib import Path
import socket
import subprocess
import time


@contextmanager
def native_search(binary, root):
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        port = sock.getsockname()[1]
    with subprocess.Popen([str(binary), "serve", "--host", "127.0.0.1", "--port", str(port)],
            cwd=root, env={"PATH": os.defpath, "HOME": str(root.parent)},
            stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL) as process:
        try:
            for _ in range(150):
                if process.poll() is not None:
                    raise RuntimeError("fixture server exited before the probe")
                try:
                    with socket.create_connection(("127.0.0.1", port), timeout=0.2):
                        break
                except OSError:
                    time.sleep(0.02)
            else:
                raise TimeoutError("fixture server not ready")

            def search(pattern, directory=".", insensitive=False):
                conn = HTTPConnection("127.0.0.1", port, timeout=5)
                try:
                    conn.request("POST", "/api/search_files", json.dumps({
                        "pattern": pattern, "dir": directory, "case_insensitive": insensitive
                    }), {"Content-Type": "application/json"})
                    response = conn.getresponse()
                    data = json.load(response)
                    if response.status != 200 or data.get("ok") is not True:
                        raise RuntimeError(f"search failed: HTTP {response.status}")
                    return data["output"]
                finally:
                    conn.close()
            yield search
        finally:
            process.terminate()
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                process.kill(); process.wait(timeout=3)


def native_binary():
    subprocess.run(["cargo", "build", "--locked", "--quiet", "--bin", "spiral-coder"], check=True)
    return Path(os.environ["CARGO_TARGET_DIR"]).resolve() / "debug" / "spiral-coder"
