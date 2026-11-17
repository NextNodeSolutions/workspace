import base64
import hashlib
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest

BINARY = Path(os.environ["WORKSPACE_TEST_BINARY"]).resolve()
MAX_PUSH_BYTES = 40 * 1024 * 1024


class SourceFixture(unittest.TestCase):
    def setUp(self):
        self.owned = tempfile.TemporaryDirectory(prefix="workspace-source-")
        self.addCleanup(self.owned.cleanup)
        self.root = Path(self.owned.name)
        (self.root / "owner").write_text("disposable committed-source fixture\n")
        self.source = self.root / "source"
        self.source.mkdir()
        self.git("init", "-q")
        self.write("tracked.txt", b"COMMITTED SYNTHETIC CONTENT\n")
        self.commit()
        self.commit_id = self.git("rev-parse", "HEAD").stdout.strip().decode()
        self.remote = self.root / "remote"
        self.remote.mkdir()
        (self.root / "bin").mkdir()
        shutil.copy(Path(__file__).with_name("source_provider.py"), self.root / "bin/boat")
        (self.root / "bin/boat").chmod(0o700)
        self.environment = {
            "PATH": f"{self.root / 'bin'}:/usr/bin:/bin",
            "HOME": str(self.remote),
            "TMPDIR": str(self.remote),
            "XDG_CONFIG_HOME": str(self.root / "config"),
            "XDG_DATA_HOME": str(self.root / "data"),
            "SOURCE_FIXTURE": str(self.root),
        }

    def git(self, *args):
        return subprocess.run(
            ["git", "-C", str(self.source), *args], check=True,
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )

    def write(self, name, content):
        path = self.source / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)

    def commit(self):
        self.git("add", "-A")
        self.git("commit", "-qm", "test: synthetic committed source")

    def job(self, *options, scope=None, command="true"):
        return subprocess.run(
            [str(BINARY), "job", "--provider", "boat", "--push",
             str(scope or self.source), "--command", command, *options],
            env=self.environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
            timeout=90,
        )

    def uploaded(self):
        archive = (self.root / "upload.gz").read_bytes()
        with tarfile.open(fileobj=io.BytesIO(archive), mode="r:gz") as source:
            names = source.getnames()
            files = {entry.name: source.extractfile(entry).read()
                     for entry in source if entry.isfile()}
        return archive, names, files

    def assert_success(self, result):
        self.assertEqual(result.returncode, 0, result.stderr.decode())

    def assert_refused(self, result):
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / "calls").exists(), result.stderr.decode())

    def assert_provenance(self, result, scope=b""):
        directory = Path(os.fsdecode(result.stdout).strip())
        provenance = json.loads((directory / "source.json").read_bytes())
        archive, names, _ = self.uploaded()
        self.assertEqual(provenance["commit"], self.git("rev-parse", "HEAD").stdout.strip().decode())
        self.assertEqual(base64.b64decode(provenance["scope_base64"]), scope)
        self.assertEqual([base64.b64decode(name) for name in provenance["members_base64"]],
                         [os.fsencode(name) for name in names])
        self.assertEqual(provenance["archive_sha256"], hashlib.sha256(archive).hexdigest())
        self.assertEqual(directory.stat().st_mode & 0o777, 0o700)
        self.assertEqual((directory / "source.json").stat().st_mode & 0o777, 0o600)
        self.assertFalse((self.root / "data/workspace/vms/fixture-vm").exists())
        self.assertNotIn(b"tracked.txt", result.stderr)
        return provenance
