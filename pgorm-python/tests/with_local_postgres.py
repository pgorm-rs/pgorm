"""Run a command using an owned local PostgreSQL cluster with verified TLS.

Set PGORM_TEST_PG_BIN to the directory containing initdb, pg_ctl and createdb.
This supports CI hosts without Docker; the installed public API needs neither.
"""

from contextlib import contextmanager
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile

from with_postgres import certificates, run


# [spec:pgorm:req:python.distribution/test]
@contextmanager
def fixture():
    binary = Path(os.environ["PGORM_TEST_PG_BIN"])
    with tempfile.TemporaryDirectory(prefix="pgorm-python-postgres-") as temporary:
        root = Path(temporary)
        data = root / "data"
        certificates(root)
        (root / "server.key").chmod(0o600)
        password = root / "password"
        password.write_text("pgorm-python-test\n")
        password.chmod(0o600)
        run(
            str(binary / "initdb"),
            "-D",
            str(data),
            "-U",
            "postgres",
            "--pwfile",
            str(password),
            "--auth-host=scram-sha-256",
            "--auth-local=trust",
            "--encoding=UTF8",
            "--no-locale",
        )
        with socket.socket() as reservation:
            reservation.bind(("127.0.0.1", 0))
            port = reservation.getsockname()[1]
        quote = lambda path: str(path).replace("'", "''")
        with (data / "postgresql.conf").open("a") as config:
            config.write(
                f"\nlisten_addresses = '127.0.0.1'\nport = {port}\n"
                f"unix_socket_directories = '{quote(root)}'\n"
                "ssl = on\n"
                f"ssl_cert_file = '{quote(root / 'server.crt')}'\n"
                f"ssl_key_file = '{quote(root / 'server.key')}'\n"
            )
        try:
            run(
                str(binary / "pg_ctl"),
                "-D",
                str(data),
                "-l",
                str(root / "postgres.log"),
                "-w",
                "-t",
                "30",
                "start",
            )
            subprocess.run(
                [
                    str(binary / "createdb"),
                    "-h",
                    "127.0.0.1",
                    "-p",
                    str(port),
                    "-U",
                    "postgres",
                    "pgorm_python",
                ],
                check=True,
                env={**os.environ, "PGPASSWORD": "pgorm-python-test"},
            )
            yield {
                **os.environ,
                "PGORM_TEST_DSN": f"postgres://postgres:pgorm-python-test@127.0.0.1:{port}/pgorm_python?sslmode=disable",
                "PGORM_TEST_CA": str(root / "ca.pem"),
            }
        finally:
            if (data / "postmaster.pid").exists():
                run(
                    str(binary / "pg_ctl"),
                    "-D",
                    str(data),
                    "-w",
                    "-m",
                    "immediate",
                    "stop",
                )


if __name__ == "__main__":
    if len(sys.argv) < 2:
        raise SystemExit("usage: with_local_postgres.py COMMAND [ARG ...]")
    with fixture() as environment:
        status = subprocess.run(sys.argv[1:], env=environment).returncode
    raise SystemExit(status)
