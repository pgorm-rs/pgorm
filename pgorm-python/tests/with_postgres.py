"""Run a command against a disposable PostgreSQL server with verified TLS.

Usage: python with_postgres.py python -m unittest discover -s pgorm-python/tests
Requires Docker and OpenSSL. The public package has neither dependency.
"""

from contextlib import contextmanager
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import uuid

IMAGE = "postgres:16.13-bookworm@sha256:472efd9a66f2b2f1a5aeb18b28de74332e6ef88c2b93a1a5d812fb6db67a5f60"


def run(*command, input=None):
    return subprocess.run(command, input=input, text=True, check=True, capture_output=True).stdout.strip()


def certificates(root):
    run("openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "2",
        "-subj", "/CN=pgorm-python-test-ca", "-addext", "basicConstraints=critical,CA:TRUE",
        "-keyout", str(root / "ca.key"), "-out", str(root / "ca.pem"))
    run("openssl", "req", "-newkey", "rsa:2048", "-nodes", "-subj", "/CN=localhost",
        "-keyout", str(root / "server.key"), "-out", str(root / "server.csr"))
    (root / "server.ext").write_text(
        "subjectAltName=DNS:localhost,IP:127.0.0.1\n"
        "basicConstraints=critical,CA:FALSE\n"
        "keyUsage=critical,digitalSignature,keyEncipherment\n"
        "extendedKeyUsage=serverAuth\n"
    )
    run("openssl", "x509", "-req", "-in", str(root / "server.csr"),
        "-CA", str(root / "ca.pem"), "-CAkey", str(root / "ca.key"), "-CAcreateserial",
        "-days", "2", "-extfile", str(root / "server.ext"), "-out", str(root / "server.crt"))


def enable_tls(name, root):
    for filename in ("server.crt", "server.key"):
        run("docker", "cp", str(root / filename), f"{name}:/var/lib/postgresql/data/{filename}")
    run("docker", "exec", name, "chown", "postgres:postgres",
        "/var/lib/postgresql/data/server.crt", "/var/lib/postgresql/data/server.key")
    run("docker", "exec", name, "chmod", "600", "/var/lib/postgresql/data/server.key")
    run("docker", "exec", "-i", name, "psql", "-v", "ON_ERROR_STOP=1", "-U", "postgres", "-d", "pgorm_python",
        input="ALTER SYSTEM SET ssl = 'on';\n"
              "ALTER SYSTEM SET ssl_cert_file = 'server.crt';\n"
              "ALTER SYSTEM SET ssl_key_file = 'server.key';\n"
              "SELECT pg_reload_conf();\n")
    deadline = time.monotonic() + 10
    while run("docker", "exec", name, "psql", "-At", "-U", "postgres", "-d", "pgorm_python", "-c", "SHOW ssl") != "on":
        if time.monotonic() >= deadline:
            raise RuntimeError("PostgreSQL TLS configuration did not become active")
        time.sleep(0.05)


# [spec:pgorm:req:python.connections/test]
@contextmanager
def fixture():
    name = f"pgorm-python-{uuid.uuid4().hex[:12]}"
    with tempfile.TemporaryDirectory(prefix="pgorm-python-tls-") as directory:
        root = Path(directory)
        certificates(root)
        run("docker", "run", "--detach", "--rm", "--name", name,
            "--label", "pgorm.python.tests=true", "-e", "POSTGRES_PASSWORD=pgorm-python-test",
            "-e", "POSTGRES_DB=pgorm_python", "-p", "127.0.0.1::5432", IMAGE)
        try:
            deadline = time.monotonic() + 45
            while True:
                ready = subprocess.run(["docker", "exec", name, "pg_isready", "-h", "127.0.0.1",
                                        "-U", "postgres", "-d", "pgorm_python"], capture_output=True)
                if ready.returncode == 0:
                    break
                if time.monotonic() >= deadline:
                    raise RuntimeError("PostgreSQL did not become ready: " + run("docker", "logs", name))
                time.sleep(0.1)
            enable_tls(name, root)
            port = run("docker", "port", name, "5432/tcp").rsplit(":", 1)[1]
            yield {
                **os.environ,
                "PGORM_TEST_DSN": f"postgres://postgres:pgorm-python-test@127.0.0.1:{port}/pgorm_python?sslmode=disable",
                "PGORM_TEST_CA": str(root / "ca.pem"),
            }
        finally:
            run("docker", "stop", "--time", "2", name)


if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit("usage: with_postgres.py COMMAND [ARG ...]")
    with fixture() as environment:
        status = subprocess.run(sys.argv[1:], env=environment).returncode
    raise SystemExit(status)
