"""Publication policy tests use throwaway git history and synthetic TOML rules."""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

_SPEC = importlib.util.spec_from_file_location('publish_checker', Path(__file__).with_name('check-publishable.py'))
p = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(p)


class PublishTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.repo = Path(self.tmp.name) / 'repo'
        self.repo.mkdir()
        self.env = {k: v for k, v in os.environ.items() if not k.startswith('GIT_')}
        self.env.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_SYSTEM=os.devnull)
        self.git('init', '-q', '-b', 'main')
        self.git('config', 'user.name', 'Example')
        self.git('config', 'user.email', 'role@example.invalid')
        self.commit('README.md', 'public root\n')
        self.base = self.git('rev-parse', 'HEAD').strip()
        self.rules_file = Path(self.tmp.name) / 'rules.toml'
        self.rules_file.write_text(f'''public_root = "{self.base}"
public_name = "Example"
public_email = "role@example.invalid"
hidden_exact = ["local.md"]
hidden_prefixes = ["local/"]
[[deny]]
name = "restricted"
pattern = "restricted"
ignore_case = true
[[allow]]
scope = "filename"
pattern = '^tools/check-restricted\\.py$'
reason = "input checker filename only"
[[allow]]
scope = "all"
pattern = "unrestricted"
reason = "ordinary word"
''')
        self.rules = p.load_rules(self.rules_file)

    def git(self, *args):
        return subprocess.run(['git', '-C', str(self.repo), *args], check=True,
                              capture_output=True, text=True, env=self.env).stdout

    def commit(self, path, content, message='change'):
        target = self.repo / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(content)
        self.git('add', '-A')
        self.git('commit', '-qm', message)

    def commit_bytes(self, path, content, message='binary'):
        target = self.repo / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
        self.git('add', '-A')
        self.git('commit', '-qm', message)

    def check(self, content=False):
        with contextlib.redirect_stdout(io.StringIO()):
            return p.check(self.repo, self.base, 'HEAD', self.rules, content)

    def test_clean_in_both_modes(self):
        self.commit('example.py', 'unrestricted\n')
        self.assertEqual(self.check(), [])
        self.assertEqual(self.check(True), [])

    def test_content_deny_in_both_modes(self):
        self.commit('example.py', 'RESTRICTED\n')
        for content in (True, False):
            self.assertTrue(any('added-content deny' in x for x in self.check(content)))

    def test_binary_content_deny_in_both_modes(self):
        # A diff shows a binary change as one "Binary files differ" line, so
        # the denied text inside the new bytes must be read from the blob.
        self.commit_bytes('docs/blob.bin', b'\x00\xff%PDF RESTRICTED\x00\x01')
        for content in (True, False):
            self.assertTrue(any('binary' in x and 'docs/blob.bin' in x and 'restricted' in x
                                for x in self.check(content)), self.check(content))

    def test_modified_binary_is_scanned(self):
        self.commit_bytes('docs/blob.bin', b'\x00clean\x00')
        self.base = self.git('rev-parse', 'HEAD').strip()
        self.rules['public_root'] = self.git('rev-list', '--max-parents=0', 'HEAD').strip()
        self.commit_bytes('docs/blob.bin', b'\x00now restricted\x00')
        for content in (True, False):
            self.assertTrue(any('binary' in x for x in self.check(content)))

    def test_clean_binary_passes(self):
        self.commit_bytes('docs/blob.bin', b'\x00\xffunrestricted\x00')
        self.assertEqual(self.check(), [])
        self.assertEqual(self.check(True), [])

    def test_filename_allow_never_covers_content(self):
        self.commit('tools/check-restricted.py', 'clean\n')
        self.assertEqual(self.check(True), [])
        self.commit('tools/check-restricted.py', 'tools/check-restricted.py\n')
        self.assertTrue(any('added-content' in x for x in self.check(True)))

    def test_filename_allow_is_tools_only(self):
        self.commit('other/check-restricted.py', 'clean\n')
        self.assertTrue(any('filename' in x for x in self.check(True)))

    def test_hidden_exact_and_prefix(self):
        self.commit('local.md', 'clean\n')
        self.commit('local/nested.md', 'clean\n')
        self.assertTrue(any('hidden paths' in x for x in self.check(True)))

    def test_hidden_prefix_cannot_be_replaced_by_a_symlink(self):
        os.symlink('README.md', self.repo / 'local')
        self.git('add', 'local')
        self.git('commit', '-qm', 'link')
        self.assertTrue(any('hidden paths' in x for x in self.check(True)))

    def test_hidden_paths_with_quoted_characters_cannot_escape(self):
        self.commit('local/new\nline.md', 'clean\n')
        self.assertTrue(any('hidden paths' in x for x in self.check(True)))

    def test_default_rules_are_relative_to_git_common_directory(self):
        policy = self.repo.parent / 'openxcvr/lib/publish-rules.toml'
        policy.parent.mkdir(parents=True)
        policy.write_bytes(self.rules_file.read_bytes())
        self.commit('example.py', 'clean\n')
        linked = self.repo / '.worktrees/feature'
        self.git('worktree', 'add', '-qb', 'feature', str(linked))
        with contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(p.main(['--content', str(linked), self.base, 'HEAD']), 0)

    def test_full_checks_transient_content(self):
        self.commit('example.py', 'restricted\n')
        self.commit('example.py', 'clean\n')
        self.assertEqual(self.check(True), [])
        self.assertTrue(any('added-content' in x for x in self.check()))

    def test_full_checks_message_and_identity(self):
        self.git('config', 'user.name', 'Other')
        self.commit('example.py', 'clean\n', 'restricted')
        self.assertEqual(self.check(True), [])
        self.assertTrue(any('identity' in x for x in self.check()))
        self.assertTrue(any('message deny' in x for x in self.check()))

    def test_full_checks_merges(self):
        self.git('checkout', '-qb', 'side')
        self.commit('side.py', 'side\n')
        self.git('checkout', '-q', 'main')
        self.commit('main.py', 'main\n')
        self.git('merge', '--no-ff', '-qm', 'combine', 'side')
        self.assertTrue(any('is a merge' in x for x in self.check()))

    def test_wrong_root(self):
        self.rules['public_root'] = '0' * 40
        self.assertTrue(any('roots' in x for x in self.check(True)))

    def test_missing_and_invalid_rules_name_flag(self):
        for text in ('', 'deny = [', 'public_root = 1'):
            self.rules_file.write_text(text)
            with self.assertRaisesRegex(ValueError, '--rules'):
                p.load_rules(self.rules_file)
        self.rules_file.unlink()
        with self.assertRaisesRegex(ValueError, '--rules'):
            p.load_rules(self.rules_file)

    def test_cli_missing_rules_fails(self):
        with contextlib.redirect_stderr(io.StringIO()) as stderr:
            result = p.main(['--rules', str(self.rules_file.parent / 'absent'),
                             '--content', str(self.repo), self.base, 'HEAD'])
        self.assertEqual(result, 1)
        self.assertIn('--rules', stderr.getvalue())


if __name__ == '__main__':
    unittest.main()
