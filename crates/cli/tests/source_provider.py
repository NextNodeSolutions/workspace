#!/usr/bin/python3
import json
import os
from pathlib import Path
import subprocess
import sys

root = Path(os.environ["SOURCE_FIXTURE"])
arguments = sys.argv[1:]
with (root / "calls").open("a") as calls:
    calls.write(arguments[0] + "\n")
operation = arguments[0]
if operation == "env":
    toggles = {"passGithub": False, "passSecrets": False,
               "passAgentsCredentials": False, "passSandboxCredentials": True}
    print(json.dumps({"environments": [{"name": "main", "isDefault": False,
                                      "safeForThirdParties": False,
                                      **toggles, "effective": toggles}]}))
elif operation == "new":
    print(json.dumps({"id": "fixture-vm"}))
elif operation == "info":
    print(json.dumps({"sandbox": {"state": "ready"}}))
elif operation == "ssh":
    payload = sys.stdin.buffer.read()
    if payload.startswith(b"\x1f\x8b"):
        (root / "upload.gz").write_bytes(payload)
    script = arguments[-1]
    remote_environment = {name: os.environ[name] for name in ("PATH", "HOME", "TMPDIR")}
    result = subprocess.run(["bash", "-c", script], input=payload, env=remote_environment)
    sys.exit(result.returncode)
elif operation in ("delete", "stop"):
    print("{}")
else:
    raise SystemExit("unexpected fake operation")
