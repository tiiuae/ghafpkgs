# SPDX-FileCopyrightText: 2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{
  lib,
  pkgs,
  crane,
}:
let
  craneLib = crane.mkLib pkgs;

  commonArgs = {
    src = ./.;
    strictDeps = true;

    pname = "ghaf-qemu-mplex";
    version = "0.1.0";

    CARGO_BUILD_INCREMENTAL = "false";
    RUST_BACKTRACE = "1";
  };

  cargoArtifacts = craneLib.buildDepsOnly commonArgs;

  cargoTest = craneLib.cargoTest (commonArgs // { inherit cargoArtifacts; });

  cargoClippy = craneLib.cargoClippy (
    commonArgs
    // {
      inherit cargoArtifacts;
      cargoClippyExtraArgs = "--all-targets -- --deny warnings";
    }
  );

  pythonWithQmp = pkgs.python3.withPackages (ps: [ ps.qemu-qmp ]);

  qmpMuxMockedQemuTest =
    pkgs.runCommand "ghaf-qemu-mplex-qmp-mux-mocked-qemu-test"
      {
        nativeBuildInputs = [
          ghaf-qemu-mplex
          pythonWithQmp
        ];
      }
      ''
            set -euo pipefail

            workdir="$(mktemp -d)"
            trap 'rm -rf "$workdir"' EXIT

            cat > "$workdir/test.py" <<'PY'
        import asyncio
        import contextlib
        import json
        import pathlib
        import socket
        import subprocess
        import sys

        from qemu.qmp.message import Message


        async def wait_for_path(path: pathlib.Path, timeout: float) -> None:
            deadline = asyncio.get_running_loop().time() + timeout
            while not path.exists():
                if asyncio.get_running_loop().time() >= deadline:
                    raise TimeoutError(f"timed out waiting for socket: {path}")
                await asyncio.sleep(0.05)


        async def handle_qemu_connection(
            reader: asyncio.StreamReader, writer: asyncio.StreamWriter
        ) -> None:
            greeting = {
                "QMP": {
                    "version": {
                        "qemu": {"major": 9, "minor": 2, "micro": 0},
                        "package": "mocked",
                    },
                    "capabilities": [],
                }
            }
            writer.write((json.dumps(greeting) + "\n").encode())
            await writer.drain()

            first_line = await reader.readline()
            assert first_line, "ghaf-qemu-mplex disconnected before handshake"
            first_msg = json.loads(first_line)
            assert (
                first_msg.get("execute") == "qmp_capabilities"
            ), f"unexpected handshake command: {first_msg}"

            handshake_reply = {"return": {}}
            if "id" in first_msg:
                handshake_reply["id"] = first_msg["id"]
            writer.write((json.dumps(handshake_reply) + "\n").encode())
            await writer.drain()

            while True:
                line = await reader.readline()
                if not line:
                    break

                request = json.loads(line)
                command = request.get("execute")
                if command == "qmp_capabilities":
                    response = {"return": {}}
                elif command == "query-status":
                    response = {
                        "return": {
                            "status": "running",
                            "running": True,
                            "singlestep": False,
                        }
                    }
                else:
                    response = {
                        "error": {
                            "class": "CommandNotFound",
                            "desc": f"unsupported command: {command}",
                        }
                    }

                if "id" in request:
                    response["id"] = request["id"]

                writer.write((json.dumps(response) + "\n").encode())
                await writer.drain()

            writer.close()
            await writer.wait_closed()


        async def main() -> None:
            mplex_bin = pathlib.Path(sys.argv[1])
            qemu_socket = pathlib.Path(sys.argv[2])
            mux_socket = pathlib.Path(sys.argv[3])

            for sock in (qemu_socket, mux_socket):
                with contextlib.suppress(FileNotFoundError):
                    sock.unlink()

            mplex = subprocess.Popen([str(mplex_bin), str(qemu_socket), str(mux_socket)])
            await asyncio.sleep(0.5)
            server = await asyncio.start_unix_server(
                handle_qemu_connection, path=str(qemu_socket)
            )
            try:
                await wait_for_path(mux_socket, timeout=5.0)

                def run_client_roundtrip(socket_path: str):
                    with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
                        sock.settimeout(5.0)
                        sock.connect(socket_path)

                        stream = sock.makefile("rwb", buffering=0)

                        def recv_msg() -> dict:
                            line = stream.readline()
                            if not line:
                                raise RuntimeError("unexpected EOF from ghaf-qemu-mplex")
                            return dict(Message(line.rstrip(b"\n")))

                        def send_msg(payload: dict) -> None:
                            stream.write(bytes(Message(payload)) + b"\n")
                            stream.flush()

                        greeting = recv_msg()
                        assert "QMP" in greeting, f"unexpected greeting payload: {greeting!r}"

                        send_msg({"execute": "qmp_capabilities"})
                        caps = recv_msg()
                        assert "return" in caps, f"unexpected capabilities payload: {caps!r}"

                        send_msg({"execute": "query-status"})
                        return recv_msg()

                status = await asyncio.wait_for(
                    asyncio.to_thread(run_client_roundtrip, str(mux_socket)),
                    timeout=15.0,
                )

                assert isinstance(status, dict), f"unexpected raw query-status response: {status!r}"
                assert (
                    status.get("return", {}).get("status") == "running"
                ), f"unexpected query-status raw payload: {status!r}"
            finally:
                mplex.terminate()
                try:
                    await asyncio.wait_for(asyncio.to_thread(mplex.wait), timeout=5.0)
                except TimeoutError:
                    mplex.kill()
                    await asyncio.to_thread(mplex.wait)

                server.close()
                await server.wait_closed()


        asyncio.run(main())
        PY

            "${pythonWithQmp}/bin/python" "$workdir/test.py" "${ghaf-qemu-mplex}/bin/ghaf-qemu-mplex" "$workdir/qemu.sock" "$workdir/mux.sock"

            touch "$out"
      '';

  qmpMuxVhotplugCompatTest =
    pkgs.runCommand "ghaf-qemu-mplex-qmp-mux-vhotplug-compat-test"
      {
        nativeBuildInputs = [
          ghaf-qemu-mplex
          pythonWithQmp
        ];
      }
      ''
            set -euo pipefail

            workdir="$(mktemp -d)"
            trap 'rm -rf "$workdir"' EXIT

            cat > "$workdir/test.py" <<'PY'
        import asyncio
        import contextlib
        import json
        import pathlib
        import socket
        import subprocess
        import sys

        from qemu.qmp import QMPClient
        from qemu.qmp.message import Message


        async def wait_for_path(path: pathlib.Path, timeout: float) -> None:
            deadline = asyncio.get_running_loop().time() + timeout
            while not path.exists():
                if asyncio.get_running_loop().time() >= deadline:
                    raise TimeoutError(f"timed out waiting for socket: {path}")
                await asyncio.sleep(0.05)


        async def handle_qemu_connection(
            reader: asyncio.StreamReader, writer: asyncio.StreamWriter
        ) -> None:
            greeting = {
                "QMP": {
                    "version": {
                        "qemu": {"major": 9, "minor": 2, "micro": 0},
                        "package": "mocked",
                    },
                    "capabilities": [],
                }
            }
            writer.write((json.dumps(greeting) + "\n").encode())
            await writer.drain()

            first_line = await reader.readline()
            assert first_line, "ghaf-qemu-mplex disconnected before handshake"
            first_msg = json.loads(first_line)
            assert (
                first_msg.get("execute") == "qmp_capabilities"
            ), f"unexpected handshake command: {first_msg}"

            handshake_reply = {"return": {}}
            if "id" in first_msg:
                handshake_reply["id"] = first_msg["id"]
            writer.write((json.dumps(handshake_reply) + "\n").encode())
            await writer.drain()

            while True:
                line = await reader.readline()
                if not line:
                    break
                request = json.loads(line)
                command = request.get("execute")
                if command == "qmp_capabilities":
                    response = {"return": {}}
                elif command == "query-status":
                    response = {
                        "return": {
                            "status": "running",
                            "running": True,
                            "singlestep": False,
                        }
                    }
                else:
                    response = {
                        "error": {
                            "class": "CommandNotFound",
                            "desc": f"unsupported command: {command}",
                        }
                    }
                if "id" in request:
                    response["id"] = request["id"]
                writer.write((json.dumps(response) + "\n").encode())
                await writer.drain()

            writer.close()
            await writer.wait_closed()


        def run_handshake_only_client(socket_path: str) -> None:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as sock:
                sock.settimeout(5.0)
                sock.connect(socket_path)
                stream = sock.makefile("rwb", buffering=0)

                def recv_msg() -> dict:
                    line = stream.readline()
                    if not line:
                        raise RuntimeError("unexpected EOF from ghaf-qemu-mplex")
                    return dict(Message(line.rstrip(b"\n")))

                def send_msg(payload: dict) -> None:
                    stream.write(bytes(Message(payload)) + b"\n")
                    stream.flush()

                greeting = recv_msg()
                assert "QMP" in greeting, f"unexpected greeting payload: {greeting!r}"
                send_msg({"execute": "qmp_capabilities"})
                caps = recv_msg()
                assert "return" in caps, f"unexpected capabilities payload: {caps!r}"


        async def run_qmpclient_roundtrip(socket_path: str) -> dict:
            qmp = QMPClient("vhotplug-like")
            await qmp.connect(socket_path)
            try:
                res = await qmp.execute("query-status")
                assert isinstance(res, dict), f"unexpected query-status response type: {type(res)}"
                return res
            finally:
                await qmp.disconnect()


        async def main() -> None:
            mplex_bin = pathlib.Path(sys.argv[1])
            qemu_socket = pathlib.Path(sys.argv[2])
            mux_socket = pathlib.Path(sys.argv[3])
            mplex_log = pathlib.Path(sys.argv[4])

            for sock in (qemu_socket, mux_socket):
                with contextlib.suppress(FileNotFoundError):
                    sock.unlink()

            with mplex_log.open("w", encoding="utf-8") as logf:
                mplex = subprocess.Popen(
                    [str(mplex_bin), str(qemu_socket), str(mux_socket)],
                    stdout=logf,
                    stderr=subprocess.STDOUT,
                )

            server = await asyncio.start_unix_server(
                handle_qemu_connection, path=str(qemu_socket)
            )

            try:
                await wait_for_path(mux_socket, timeout=5.0)

                # Client 1: handshake and disconnect.
                await asyncio.wait_for(
                    asyncio.to_thread(run_handshake_only_client, str(mux_socket)),
                    timeout=10.0,
                )

                # Client 2: vhotplug-like QMPClient flow.
                status = await asyncio.wait_for(
                    run_qmpclient_roundtrip(str(mux_socket)),
                    timeout=10.0,
                )
                assert (
                    status.get("status") == "running"
                ), f"unexpected query-status payload from QMPClient: {status!r}"
            finally:
                mplex.terminate()
                try:
                    await asyncio.wait_for(asyncio.to_thread(mplex.wait), timeout=5.0)
                except TimeoutError:
                    mplex.kill()
                    await asyncio.to_thread(mplex.wait)
                server.close()
                await server.wait_closed()


        asyncio.run(main())
        PY

            RUST_LOG=ghaf_qemu_mplex=trace "${pythonWithQmp}/bin/python" "$workdir/test.py" "${ghaf-qemu-mplex}/bin/ghaf-qemu-mplex" "$workdir/qemu.sock" "$workdir/mux.sock" "$workdir/mplex.log"

            touch "$out"
      '';

  ghaf-qemu-mplex = craneLib.buildPackage (
    commonArgs
    // {
      inherit cargoArtifacts;

      passthru.tests = {
        inherit cargoTest cargoClippy;
        inherit qmpMuxMockedQemuTest qmpMuxVhotplugCompatTest;
      };

      meta = {
        description = "QEMU QMP multiplexer for Ghaf microVMs";
        longDescription = ''
          A QEMU QMP socket multiplexer for Ghaf that proxies a single
          microVM QMP endpoint to multiple clients while preserving
          request/response ordering and event fan-out semantics.
        '';
        homepage = "https://ghaf.dev";
        license = lib.licenses.asl20;
        platforms = lib.platforms.linux;
        mainProgram = "ghaf-qemu-mplex";
      };
    }
  );
in
ghaf-qemu-mplex
