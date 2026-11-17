import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import unittest

from source_fixture import MAX_PUSH_BYTES, SourceFixture


class SourceLimits(SourceFixture):
    def test_tar_header_overhead_is_bounded_before_provider(self):
        self.write("large", b"x" * (MAX_PUSH_BYTES - 256))
        self.commit()
        self.assert_refused(self.job())

    def test_gzip_output_is_bounded_before_provider(self):
        wrapper = self.root / "bin/gzip"
        wrapper.write_text(
            "#!/usr/bin/python3\nimport sys\nsys.stdin.buffer.read()\n"
            f"sys.stdout.buffer.write(b'x' * {MAX_PUSH_BYTES + 1})\n"
        )
        wrapper.chmod(0o700)
        self.assert_refused(self.job())

    def test_runtime_value_in_manifest_commit_is_refused(self):
        self.environment["SYNTHETIC_RUNTIME"] = self.commit_id
        self.assert_refused(self.job("--env", "SYNTHETIC_RUNTIME"))

    def test_runtime_value_in_scope_is_refused(self):
        self.write("SYNTHETIC_SCOPE/tracked", b"SAFE\n")
        self.commit()
        self.environment["SYNTHETIC_RUNTIME"] = "SYNTHETIC_SCOPE"
        result = self.job("--env", "SYNTHETIC_RUNTIME", scope=self.source / "SYNTHETIC_SCOPE")
        self.assert_refused(result)
        self.assertNotIn(b"SYNTHETIC_SCOPE", result.stderr)

    def test_missing_promisor_blob_never_invokes_transport(self):
        blob = self.git("rev-parse", "HEAD:tracked.txt").stdout.strip().decode()
        directory = Path(self.git("rev-parse", "--absolute-git-dir").stdout.decode().strip())
        (directory / "objects" / blob[:2] / blob[2:]).unlink()
        self.git("config", "remote.synthetic.url", "ext::false")
        self.git("config", "remote.synthetic.promisor", "true")
        self.git("config", "extensions.partialClone", "synthetic")
        self.git("config", "protocol.ext.allow", "always")
        self.assert_refused(self.job())

    def test_git_object_replacements_cannot_change_pinned_bytes(self):
        replacement = subprocess.run(
            ["git", "-C", str(self.source), "hash-object", "-w", "--stdin"],
            input=b"REPLACEMENT BYTES\n", stdout=subprocess.PIPE, check=True,
        ).stdout.strip().decode()
        original = self.git("rev-parse", "HEAD:tracked.txt").stdout.strip().decode()
        self.git("replace", original, replacement)
        self.assert_success(self.job())
        self.assertEqual(self.uploaded()[2]["tracked.txt"], b"COMMITTED SYNTHETIC CONTENT\n")

    def test_runtime_setup_collection_and_cleanup_keep_existing_contract(self):
        setup = self.root / "setup.sh"
        setup.write_text('test -f tracked.txt && test -z "${SYNTHETIC_RUNTIME+x}" && printf ready > setup.result\n')
        self.environment["SYNTHETIC_RUNTIME"] = "SYNTHETIC COMMAND VALUE"
        result = self.job("--setup", str(setup), "--env", "SYNTHETIC_RUNTIME",
                          "--artifact", "report", command='test -f setup.result; printf "%s" "$SYNTHETIC_RUNTIME"; printf result > report; exit 7')
        self.assertEqual(result.returncode, 7, result.stderr.decode())
        directory = Path(os.fsdecode(result.stdout).strip())
        self.assertEqual((directory / "setup.status").read_bytes(), b"0")
        self.assertEqual((directory / "command.status").read_bytes(), b"7")
        self.assertEqual((directory / "command.log").read_bytes(), b"***")
        with tarfile.open(fileobj=io.BytesIO((directory / "artifacts.tar").read_bytes())) as archive:
            self.assertEqual(archive.extractfile("./report").read(), b"result")
        self.assert_provenance(result)

    def test_workdir_leaf_symlink_is_never_followed(self):
        outside = self.root / "outside"
        outside.mkdir()
        (self.remote / "job").symlink_to(outside)
        result = self.job()
        self.assertNotEqual(result.returncode, 0)
        self.assertEqual(list(outside.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
