import os
from pathlib import Path
import subprocess
import unittest

from source_fixture import SourceFixture


class SourceEdges(SourceFixture):
    def test_scope_unusual_names_and_all_attributes_are_literal(self):
        name = "nested area/line\n-tab\t-$name.txt"
        self.write(name, b"$Format:%H$\n")
        self.write("nested area/.gitattributes", b"* export-ignore export-subst\n")
        self.commit()
        self.write("nested area/.gitattributes", b"* -export-ignore -export-subst\n")
        info = Path(self.git("rev-parse", "--absolute-git-dir").stdout.decode().strip()) / "info"
        (info / "attributes").write_bytes(b"* export-ignore export-subst\n")
        global_attributes = self.root / "global-attributes"
        global_attributes.write_bytes(b"* export-ignore export-subst\n")
        self.git("config", "core.attributesFile", str(global_attributes))
        result = self.job(scope=self.source / "nested area")
        self.assert_success(result)
        _, names, files = self.uploaded()
        self.assertEqual(names, [".gitattributes", "line\n-tab\t-$name.txt"])
        self.assertEqual(files["line\n-tab\t-$name.txt"], b"$Format:%H$\n")
        self.assert_provenance(result, b"nested area")

    def test_scope_must_be_a_committed_directory(self):
        result = self.job(scope=self.source / "tracked.txt")
        self.assert_refused(result)

    def test_non_utf8_and_long_names_survive_with_exact_manifest(self):
        self.write(os.fsdecode(b"name-\xff"), b"BINARY NAME\n")
        self.write("long/" + "n" * 200, b"LONG NAME\n")
        self.commit()
        result = self.job()
        self.assert_success(result)
        self.assert_provenance(result)

    def test_safe_links_preserve_targets_and_executable_mode(self):
        self.write("dir/executable", b"#!/bin/sh\nexit 0\n")
        (self.source / "dir/executable").chmod(0o755)
        (self.source / "alias").symlink_to("dir")
        (self.source / "link").symlink_to("alias/executable")
        self.commit()
        result = self.job(command="test -L link && test -x dir/executable && ./link")
        self.assert_success(result)
        self.assert_provenance(result)

    def test_chained_parent_escape_is_refused(self):
        self.write("dir/file", b"SAFE\n")
        (self.source / "dir/up").symlink_to("..")
        (self.source / "escape").symlink_to("dir/up/../outside")
        self.commit()
        self.assert_refused(self.job())

    def test_existing_workdir_is_preserved_and_not_extracted_into(self):
        destination = self.remote / "job"
        destination.mkdir()
        (destination / "old").write_bytes(b"KEEP OLD\n")
        result = self.job()
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((destination / "tracked.txt").exists())
        self.assertEqual((destination / "old").read_bytes(), b"KEEP OLD\n")
        self.assertNotIn("delete", (self.root / "calls").read_text())

    def test_destination_parent_symlink_cannot_receive_source(self):
        outside = self.root / "outside"
        outside.mkdir()
        (self.remote / "alias").symlink_to(outside)
        result = self.job("--workdir", str(self.remote / "alias/new"))
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((outside / "new").exists())

    def test_head_movement_during_preparation_cannot_change_pinned_tree(self):
        self.write("tracked.txt", b"LATER COMMIT\n")
        self.commit()
        later = self.git("rev-parse", "HEAD").stdout.strip().decode()
        self.git("update-ref", "HEAD", self.commit_id)
        wrapper = self.root / "bin/git"
        wrapper.write_text(
            "#!/usr/bin/python3\nimport os, subprocess, sys\n"
            "result = subprocess.run(['/usr/bin/git', *sys.argv[1:]], stdout=subprocess.PIPE)\n"
            "if 'HEAD^{commit}' in sys.argv:\n"
            f"    subprocess.run(['/usr/bin/git', '-C', {str(self.source)!r}, 'update-ref', 'HEAD', {later!r}], check=True)\n"
            "sys.stdout.buffer.write(result.stdout)\nsys.exit(result.returncode)\n"
        )
        wrapper.chmod(0o700)
        result = self.job()
        self.assert_success(result)
        self.assertEqual(self.uploaded()[2]["tracked.txt"], b"COMMITTED SYNTHETIC CONTENT\n")
        import json
        provenance = json.loads((Path(os.fsdecode(result.stdout).strip()) / "source.json").read_bytes())
        self.assertEqual(provenance["commit"], self.commit_id)

    def test_filename_list_bound_is_enforced_before_provider(self):
        blob = self.git("rev-parse", "HEAD:tracked.txt").stdout.strip()
        records = b"".join(b"100644 " + blob + b"\t" + f"{index:05}-".encode() + b"n" * 240 + b"\0"
                           for index in range(16000))
        subprocess.run(["git", "-C", str(self.source), "update-index", "-z", "--index-info"],
                       input=records, check=True)
        self.git("commit", "-qm", "test: synthetic bounded member list")
        self.assert_refused(self.job())


if __name__ == "__main__":
    unittest.main()
