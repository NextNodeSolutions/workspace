import os
import shutil
import unittest

from source_fixture import MAX_PUSH_BYTES, SourceFixture


class CommittedUpload(SourceFixture):
    def test_untracked_agent_reports_are_excluded(self):
        self.write(".pi/acceptance.md", b"UNTRACKED SYNTHETIC REPORT\n")
        result = self.job()
        self.assert_success(result)
        _, names, _ = self.uploaded()
        self.assertEqual(names, ["tracked.txt"])

    def test_worktree_and_index_cannot_replace_committed_bytes(self):
        self.write("tracked.txt", b"STAGED SYNTHETIC CONTENT\n")
        self.write("addition.txt", b"STAGED ADDITION\n")
        self.git("add", "-A")
        self.write("tracked.txt", b"WORKTREE SYNTHETIC CONTENT\n")
        result = self.job()
        self.assert_success(result)
        _, names, files = self.uploaded()
        self.assertEqual(names, ["tracked.txt"])
        self.assertEqual(files["tracked.txt"], b"COMMITTED SYNTHETIC CONTENT\n")

    def test_missing_worktree_file_still_exports_committed_blob(self):
        (self.source / "tracked.txt").unlink()
        self.assert_success(self.job())
        self.assertEqual(self.uploaded()[2]["tracked.txt"], b"COMMITTED SYNTHETIC CONTENT\n")

    def test_ignore_rules_do_not_exclude_committed_files(self):
        self.write("committed.private", b"COMMITTED IGNORE MATCH\n")
        self.commit()
        self.write(".gitignore", b"*.private\n")
        self.commit()
        self.write("untracked.private", b"UNTRACKED IGNORED\n")
        self.assert_success(self.job())
        self.assertEqual(self.uploaded()[2]["committed.private"], b"COMMITTED IGNORE MATCH\n")
        self.assertNotIn("untracked.private", self.uploaded()[1])

    def test_replaced_parent_link_cannot_read_outside_source(self):
        self.write("nested/file.txt", b"COMMITTED NESTED\n")
        self.commit()
        outside = self.root / "outside"
        outside.mkdir()
        (outside / "file.txt").write_bytes(b"OUTSIDE SYNTHETIC CONTENT\n")
        shutil.rmtree(self.source / "nested")
        (self.source / "nested").symlink_to(outside)
        self.assert_success(self.job())
        self.assertEqual(self.uploaded()[2]["nested/file.txt"], b"COMMITTED NESTED\n")

    def test_unsafe_committed_links_fail_before_provider(self):
        for target in ("/outside", "../outside", "dir/../../outside", "cycle"):
            with self.subTest(target=target):
                link = self.source / "link"
                link.unlink(missing_ok=True)
                link.symlink_to(target)
                (self.source / "cycle").unlink(missing_ok=True)
                (self.source / "cycle").symlink_to("link")
                self.commit()
                self.assert_refused(self.job())

    def test_submodule_is_refused_before_provider(self):
        self.git("update-index", "--add", "--cacheinfo", f"160000,{self.commit_id},module")
        self.git("commit", "-qm", "test: synthetic gitlink")
        result = self.job()
        self.assert_refused(result)
        self.assertIn(b"submodule", result.stderr)

    def test_provenance_matches_stdin_and_survives_vm_cleanup(self):
        result = self.job()
        self.assert_success(result)
        self.assert_provenance(result)
        self.assertIn("delete", (self.root / "calls").read_text())

    def test_selected_runtime_value_in_committed_blob_is_refused(self):
        self.environment["SYNTHETIC_RUNTIME"] = "COMMITTED SYNTHETIC CONTENT"
        result = self.job("--env", "SYNTHETIC_RUNTIME")
        self.assert_refused(result)
        self.assertNotIn(b"COMMITTED SYNTHETIC CONTENT", result.stderr)

    def test_oversized_committed_blob_is_refused(self):
        self.write("large", b"x" * MAX_PUSH_BYTES)
        self.commit()
        self.assert_refused(self.job())


if __name__ == "__main__":
    unittest.main()
