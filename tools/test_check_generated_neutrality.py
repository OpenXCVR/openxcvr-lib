#!/usr/bin/env python3
"""Stage 1–3b qualification tests; stdlib only, no regeneration of the real repo.

Run: PYTHONDONTWRITEBYTECODE=1 python3 tools/test_check_generated_neutrality.py
Optional real source-domain smoke: KCR_NEUTRALITY_SMOKE=1 <same command>.
Task admission/regeneration use synthetic parent evidence and fake Cargo.
Finish replay uses synthetic accepted bindings and current qualifications.
Nine-file removal/restoration and both-mode preservation use temporary repos.
"""

from __future__ import annotations

import copy
import errno
import signal
import stat
import time
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HELPER = Path(__file__).with_name('check-generated-neutrality.py').resolve()
sys.dont_write_bytecode = True  # never leave tools/__pycache__ in the working-state snapshot
SPEC = importlib.util.spec_from_file_location('generated_neutrality', HELPER)
gn = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gn)
MAIN = 'crates/openxcvr-codegen/src/main.rs'
RUST = 'crates/openxcvr-codegen/src/emitters/rust/pages.rs'
ROOT = 'bindings/python/openxcvr/_generated'

# C5r2: the suite's verdict must not depend on the operator's shell. Every
# name the helper's own guard would refuse is removed from this process's
# environment before any test runs and restored afterwards; the refusal is
# covered by its dedicated tests, which set the names they exercise.
_AMBIENT_OVERRIDES = {}

# Compatibility entry points from the frozen contract-v3.1.md focused commands,
# not additional full-suite coverage.
_FROZEN_CONTRACT_ALIASES = (
    'test_partial_acquisition_and_pin_order',
    'test_task_finish_nine_and_receipt_immutability',
)
_FROZEN_CONTRACT_FOCUSED_NAMES = (
    'test_real_dirty_chmod000_private_export',
    'test_safe_siblings_and_unsafe_destination_untouched',
    'test_export_after_evidence_and_linked_worktree_removal',
    'test_export_rejects_recorded_evidence_overlap_after_deletion',
    'test_independent_format_decoder',
    'test_uid_owned_0775_metadata_without_chmod',
    'test_root_and_sticky_ancestry',
    'test_namespace_invisible_main_and_linked',
    'test_live_finalizer_and_export_excluded',
    'test_helper_killed_generator_alive',
    'test_intermediate_killed_mutating_descendant_alive',
    'test_partial_acquisition_and_pin_order',
    'test_atomic_completion_interruptions_and_conflicts',
    'test_export_does_not_restore_or_release',
    'test_unmatched_and_malformed_never_pruned',
    'test_completed_sequence_retention_and_config',
    'test_task_finish_nine_and_receipt_immutability',
)


def _suite_cases(tests):
    for test in tests:
        if isinstance(test, unittest.TestSuite):
            yield from _suite_cases(test)
        else:
            yield test


def load_tests(loader, tests, pattern):
    # Module/script collection only: unittest's dotted-method lookup bypasses
    # this hook, so the frozen focused commands can still select each alias.
    return unittest.TestSuite(test for test in _suite_cases(tests)
                              if test._testMethodName not in _FROZEN_CONTRACT_ALIASES)


def setUpModule():
    for name in gn._launch_overrides(os.environ):
        _AMBIENT_OVERRIDES[name] = os.environ.pop(name)


def tearDownModule():
    os.environ.update(_AMBIENT_OVERRIDES)
    _AMBIENT_OVERRIDES.clear()

# Small independent input fixtures, not copies of production recipe/emitter
# implementations. They have the same declared grammar but different roots.
ALL = '''#!/usr/bin/env bash
set -euo pipefail
for lang in python rust; do
    "$REPO_ROOT/tools/regen-$lang.sh" "${SPECS[@]}"
done
for lang in c; do
    case "$lang" in
        c) OUT="$REPO_ROOT/bindings/c/generated" ;;
    esac
    cargo run -q -p openxcvr-codegen -- generate --spec "$spec" --lang "$lang" --out "$OUT"
done
'''
PYTHON = '''set -euo pipefail
OUT="$REPO_ROOT/bindings/python/openxcvr/_generated"
cargo run -q -p openxcvr-codegen -- generate \\
    --spec "$spec" --lang python --out "$OUT"
'''
RUST_SCRIPT = '''set -euo pipefail
regen_spec() {
local spec="$1" crate="$2"
local out="$REPO_ROOT/crates/$crate/src/generated"
cargo run -q -p openxcvr-codegen -- generate --spec "$spec" --out "$out" --lang rust
}
for spec in "${SPECS[@]}"; do
    case "$spec" in
        cmis) regen_spec cmis synthetic-crate ;;
    esac
done
'''
MAIN_SOURCE = '''if !matches!(lang.as_str(), "rust" | "python" | "c") {}
std::fs::create_dir_all(&out)?;
let out_file = if python_mode {
    out.join("enums").join(format!("{}.py", camel_to_snake(&schema.id)))
} else {
    out.join(format!("{}.rs", schema.id))
};
std::fs::write(&out_file, &code)?;
std::fs::write(out.join("openxcvr_generated.h"), code)?;
'''
RUST_SOURCE = '''std::fs::create_dir_all(out_dir)?;
std::fs::write(out_dir.join(format!("{}.rs", page.id)), code)?;
let dir = out_dir.join(format!("v{}", rev.dirname()));
std::fs::create_dir_all(&dir)?;
std::fs::write(dir.join(format!("{}.rs", page.id)), body)?;
'''


# Executed fixtures: the fail-fast prologue first (C11), then the spec loop
# that binds `$spec` for the generator lines, as the real recipes do.
PROLOGUE = 'set -euo pipefail\n'
EXECUTED_HEADER = (PROLOGUE + 'REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"\n'
                   'SPECS=(cmis)\nfor spec in "${SPECS[@]}"; do\n')
EXECUTED_FOOTER = 'done\n'


def executed(text: str) -> str:
    """A parse fixture as an executed recipe: one prologue, `$spec` bound."""
    body = text.removeprefix('#!/usr/bin/env bash\n').removeprefix(PROLOGUE)
    return EXECUTED_HEADER + body + EXECUTED_FOOTER


class ScopeTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='neutrality-stage1-')
        self.addCleanup(self.temp.cleanup)
        self.repo = Path(self.temp.name)
        self.git('init', '-q')
        self.write('tools/regen-all.sh', ALL)
        self.write('tools/regen-python.sh', PYTHON)
        self.write('tools/regen-rust.sh', RUST_SCRIPT)
        self.write(MAIN, MAIN_SOURCE)
        self.write(RUST, RUST_SOURCE)

    def git(self, *args):
        result = subprocess.run(['git', '-C', str(self.repo), *args],
                                capture_output=True, check=True)
        return result.stdout.decode().strip()

    def write(self, relative, text):
        path = self.repo / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
        return path

    def commit(self):
        self.git('add', '.')
        self.git('-c', 'user.name=Stage1 Test', '-c', 'user.email=stage1@example.invalid',
                 'commit', '-qm', 'synthetic source/output baseline')
        return self.git('rev-parse', 'HEAD')

    def scope(self, revision=None):
        return gn.derive_scope(self.repo, revision)

    def manifest(self):
        return gn.filesystem_manifest(self.repo, self.scope()['output_rules'])

    def assertRefused(self):
        with self.assertRaises(gn.NeutralityError):
            self.scope()

    def test_source_derived_domains_and_hashes(self):
        scope = self.scope()
        self.assertEqual([r['root'] for r in scope['output_rules']], [
            'bindings/c/generated', ROOT, 'crates/synthetic-crate/src/generated'])
        self.assertTrue(scope['emitter_destinations'])
        for source in scope['scope_sources']:
            self.assertEqual(source['sha256'], hashlib.sha256(
                (self.repo / source['path']).read_bytes()).hexdigest())
        for rule in scope['output_rules']:
            self.assertEqual(rule['source_sha256'], gn.sha256(
                (self.repo / rule['source']).read_bytes()))
        self.assertEqual(self.scope(self.commit()), scope)

    def test_dynamic_new_root_and_pre_post_union(self):
        before = self.scope()
        base = self.commit()
        self.write('tools/regen-python.sh', PYTHON.replace(ROOT, 'bindings/python/new/generated'))
        after = self.scope()
        union = gn.union_rules(before, after)
        self.assertEqual(len(union), 4)
        self.assertEqual(gn.git_manifest(self.repo, base, union)['manifest'], [])
        self.write('bindings/python/new/generated/future.py', 'future')
        self.assertEqual(len(gn.filesystem_manifest(self.repo, union)['manifest']), 1)
        self.write('bindings/python/new/generated/at-base.py', 'base content in future domain')
        future_base = self.commit()
        recovered = gn.git_manifest(self.repo, future_base, union)
        self.assertEqual(len(recovered['manifest']), 2)
        self.assertEqual(self.scope(base), before)

    def test_same_root_rule_names_the_current_script(self):
        # C9: when the post-edit recipe keeps a prelaunch root, the recorded
        # rule names the script that runs, not the prelaunch revision.
        before = self.scope(self.commit())
        script = self.write('tools/regen-python.sh', '# edited recipe, same root\n' + PYTHON)
        after = self.scope()
        current = gn.sha256(script.read_bytes())
        self.assertNotEqual([r for r in before['output_rules'] if r['root'] == ROOT][0]['source_sha256'],
                            current)
        union = gn.union_rules(before, after)
        self.assertEqual([r['root'] for r in union], [r['root'] for r in before['output_rules']])
        self.assertEqual([r for r in union if r['root'] == ROOT],
                         [dict(root=ROOT, rule='recursive-files', source='tools/regen-python.sh',
                               source_sha256=current)])
        self.assertEqual(union, after['output_rules'])
        # A root only the prelaunch scope declares is still kept.
        moved = self.write('tools/regen-python.sh', PYTHON.replace(ROOT, 'bindings/python/new/generated'))
        union = gn.union_rules(before, self.scope())
        self.assertIn([r for r in before['output_rules'] if r['root'] == ROOT][0], union)
        self.assertEqual([r for r in union if r['root'] == 'bindings/python/new/generated'][0]['source_sha256'],
                         gn.sha256(moved.read_bytes()))

    def test_complete_manifest_added_deleted_modified_renamed_ignored(self):
        self.write(ROOT + '/tracked.py', 'base bytes')
        self.write(ROOT + '/deleted.py', 'to delete')
        self.write(ROOT + '/renamed.py', 'to rename')
        self.write('.gitignore', '*.ignored\n')
        scope = self.scope()
        base = self.commit()
        before = gn.git_manifest(self.repo, base, scope['output_rules'])
        self.assertEqual(before, self.manifest())
        self.write(ROOT + '/tracked.py', 'changed bytes')
        (self.repo / ROOT / 'deleted.py').unlink()
        (self.repo / ROOT / 'renamed.py').rename(self.repo / ROOT / 'new_name.py')
        self.write(ROOT + '/tracked_added.py', 'added and staged')
        self.git('add', ROOT + '/tracked_added.py')
        self.write(ROOT + '/untracked.py', 'untracked')
        self.write(ROOT + '/nested/hidden.ignored', 'ignored')
        self.assertIn('??', self.git('status', '--short', '--untracked-files=all'))
        self.assertTrue(self.git('check-ignore', ROOT + '/nested/hidden.ignored'))
        after = self.manifest()
        self.assertEqual({e['path'] for e in after['manifest']}, {
            ROOT + '/' + p for p in ('tracked.py', 'new_name.py', 'tracked_added.py',
                                     'untracked.py', 'nested/hidden.ignored')})
        for entry in after['manifest']:
            self.assertEqual(entry, {'path': entry['path'], 'kind': 'file',
                                    'sha256': gn.sha256((self.repo / entry['path']).read_bytes())})
        self.assertNotEqual(before['manifest_sha256'], after['manifest_sha256'])
        self.assertEqual(before, gn.git_manifest(self.repo, base, scope['output_rules']))

    def test_manifest_hash_encoding_order_and_content(self):
        self.write(ROOT + '/z.py', 'z')
        self.write(ROOT + '/a.py', 'a')
        manifest = self.manifest()
        entries = manifest['manifest']
        self.assertEqual([e['path'] for e in entries], [ROOT + '/a.py', ROOT + '/z.py'])
        encoded = json.dumps(entries, sort_keys=True, separators=(',', ':'),
                             ensure_ascii=True).encode('utf-8')
        self.assertEqual(manifest['manifest_sha256'], hashlib.sha256(encoded).hexdigest())
        self.assertEqual(manifest, self.manifest())
        (self.repo / ROOT / 'a.py').rename(self.repo / ROOT / 'b.py')
        self.assertNotEqual(manifest['manifest_sha256'], self.manifest()['manifest_sha256'])

    def test_full_filename_domain_and_handwritten_exclusions(self):
        for root in (ROOT, 'bindings/c/generated', 'crates/synthetic-crate/src/generated'):
            self.write(root + '/unknown-future/nested/new.any-extension', 'output')
            # No exception is declared for a README *inside* a generated root.
            self.write(root + '/README.md', 'inside owned domain')
        before = self.manifest()
        for path in ('bindings/README.md', 'bindings/python/README.md',
                     'bindings/python/openxcvr/__init__.py', 'bindings/c/wrapper.c',
                     'crates/synthetic-crate/src/lib.rs'):
            self.write(path, 'handwritten')
        self.assertEqual(before, self.manifest())
        self.assertEqual(len(before['manifest']), 6)

    def test_empty_and_deleted_domain(self):
        self.assertEqual(self.manifest()['manifest'], [])
        self.write(ROOT + '/only.py', 'only')
        previous = self.manifest()
        shutil.rmtree(self.repo / ROOT)
        current = self.manifest()
        self.assertEqual(current['manifest'], [])
        self.assertNotEqual(previous, current)

    def test_symlink_root_ancestor_file_and_dangling_output(self):
        for relative in (ROOT, 'bindings/python', ROOT + '/dangling.py'):
            with self.subTest(relative=relative):
                path = self.repo / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                if path.is_dir():
                    shutil.rmtree(path)
                path.symlink_to('/definitely-missing-neutrality-target')
                with self.assertRaises(gn.NeutralityError):
                    self.manifest()
                path.unlink()

    def test_nonregular_output_and_file_root(self):
        path = self.write(ROOT, 'not a directory')
        with self.assertRaises(gn.NeutralityError):
            self.manifest()
        path.unlink()
        path.mkdir()
        os.mkfifo(path / 'fifo')
        with self.assertRaises(gn.NeutralityError):
            self.manifest()

    def test_git_symlink_and_submodule_refused(self):
        scope = self.scope()
        path = self.repo / ROOT / 'link.py'
        path.parent.mkdir(parents=True, exist_ok=True)
        path.symlink_to('missing')
        base = self.commit()
        with self.assertRaises(gn.NeutralityError):
            gn.git_manifest(self.repo, base, scope['output_rules'])
        path.unlink()
        self.git('rm', '--cached', ROOT + '/link.py')
        self.git('update-index', '--add', '--cacheinfo', '160000', base, ROOT + '/module')
        tree = self.git('write-tree')
        with self.assertRaises(gn.NeutralityError):
            gn.git_manifest(self.repo, tree, scope['output_rules'])

    def test_git_symlink_ancestor_refused(self):
        scope = self.scope()
        (self.repo / 'bindings').symlink_to('missing')
        base = self.commit()
        with self.assertRaises(gn.NeutralityError):
            gn.git_manifest(self.repo, base, scope['output_rules'])

    def test_escaping_ambiguous_duplicate_and_unresolved_roots(self):
        for root in ('../generated', '/tmp/generated', 'bindings',
                     'bindings/python/$future/generated', 'bindings//generated',
                     'bindings/./generated', 'bindings/.git/generated'):
            with self.subTest(root=root):
                self.write('tools/regen-python.sh', PYTHON.replace(ROOT, root))
                self.assertRefused()
        self.write('tools/regen-python.sh', PYTHON.replace(ROOT, 'bindings/c/generated'))
        self.assertRefused()
        self.write('tools/regen-python.sh', PYTHON + PYTHON)
        self.assertRefused()

    def test_overlap_and_unknown_rule(self):
        rules = self.scope()['output_rules']
        for extra in (dict(rules[0]), dict(rules[0], root='bindings/c/generated/nested/generated'),
                      dict(rules[0], rule='unknown')):
            with self.assertRaises(gn.NeutralityError):
                gn.filesystem_manifest(self.repo, rules + [extra])

    def test_shell_unparsed_source_and_invocation_fail_closed(self):
        for text in (PYTHON + 'touch "$OUT/side-file"\n',
                     PYTHON.replace('--out "$OUT"', '--out "$OTHER"'),
                     PYTHON.replace('--lang python', '--lang java'),
                     PYTHON.replace('cargo run', 'eval cargo run'),
                     PYTHON.split('cargo run')[0],
                     PYTHON + 'source extra.sh\n',
                     PYTHON + 'done\n',
                     PYTHON + 'for spec in "${SPECS[@]}"; do\n'):
            with self.subTest(text=text):
                self.write('tools/regen-python.sh', text)
                self.assertRefused()

    def test_bounded_language_list_recognizer(self):
        # R1: the emitter language guard is read as bounded tokens. The
        # canonical emitter form and the supported whitespace/order variants
        # are accepted; malformed or ambiguous language syntax refuses.
        rest = MAIN_SOURCE.split('\n', 1)[1]
        accepted = ('if !matches!(lang.as_str(), "rust" | "python" | "c") {}',
                    'if !matches!(\n    lang.as_str(),\n    "c"|"rust"|"python"\n) {}',
                    'if !matches!( lang.as_str() , "python" | "c" | "rust" ) {}')
        for text in accepted:
            with self.subTest(text=text):
                self.write(MAIN, text + '\n' + rest)
                self.assertTrue(self.scope()['output_rules'])
        malformed = ('if !matches!(lang.as_str(), "rust" | "python" | "c"',
                     'if !matches!(lang.as_str(), "rust" | "python" | "c" |) {}',
                     'if !matches!(lang.as_str(), "rust" "python" "c") {}',
                     'if !matches!(lang.as_str(), "rust" | "python" | "c" | "rust") {}',
                     'if !matches!(lang.as_str(), "rust" | "python" | "C") {}',
                     'if !matches!(lang.as_str(), "rust" | "python" | c) {}',
                     'if !matches!(lang.as_str(), ) {}')
        for text in malformed:
            with self.subTest(text=text):
                self.write(MAIN, text + '\n' + rest)
                with self.assertRaisesRegex(gn.NeutralityError,
                                            'unresolved emitter language coverage'):
                    self.scope()

    def test_language_guard_pathological_input_stays_bounded(self):
        # The saved alert-38 reproduction: ambiguous whitespace partitions with
        # the closing ')' absent. The old repeated-subexpression regex grew
        # superlinearly; the bounded scan refuses immediately. The ceiling is a
        # generous wall-clock bound, not a machine-specific timing threshold.
        self.write(MAIN, 'if !matches!(lang.as_str(),"a"' + '  "a"' * 4000 + '!')
        start = time.perf_counter()
        with self.assertRaisesRegex(gn.NeutralityError,
                                    'unresolved emitter language coverage'):
            self.scope()
        self.assertLess(time.perf_counter() - start, 10)

    def test_inert_echo_grammar_accepts_literals_and_refuses_execution(self):
        # R2: the recipe grammar accepts actual progress messages, literal text
        # and simple variable references, and refuses every execution form
        # before a recipe can run.
        recipe = PYTHON
        marker = '/tmp/neutrality-inert-echo-marker'
        accepted = ('echo "Done."',
                    'echo "\u2192 $spec \u2192 $OUT"',
                    'echo "  fmt \u2192 $crate"',
                    'echo "Unknown spec: $spec; expected cmis | sff-8636 | sff-8472" >&2',
                    'echo "\u2192 $spec ($lang) \u2192 $OUT"')
        for line in accepted:
            with self.subTest(line=line):
                gn._shell_recipe(recipe + line + '\n', 'tools/regen-python.sh')
        refused = ('echo "$(touch ' + marker + ')"',
                   'echo "`touch ' + marker + '`"',
                   'echo "${SPECS[@]}"',
                   'echo "$((1 + 2))"',
                   'echo "$<' + marker + '"',
                   "echo 'literal single quotes'",
                   'printf "Done.\\n"',
                   'echo "$OUT" > ' + marker,
                   'echo "a" | tee ' + marker,
                   'echo "a" && touch ' + marker,
                   'echo "a" 2>' + marker)
        for line in refused:
            with self.subTest(line=line), self.assertRaises(gn.NeutralityError):
                gn._shell_recipe(recipe + line + '\n', 'tools/regen-python.sh')

    def test_child_dispatch_binds_its_own_language_loop(self):
        first, second = ALL.split('for lang in c; do')
        swapped = 'set -euo pipefail\nfor lang in c; do' + second + first
        self.assertEqual(gn._shell_recipe(swapped, 'tools/regen-all.sh')[1],
                         ['tools/regen-python.sh', 'tools/regen-rust.sh'])
        dispatch = '    "$REPO_ROOT/tools/regen-$lang.sh" "${SPECS[@]}"\n'
        for text in (ALL.replace(dispatch, '') + dispatch,
                     ALL.replace(dispatch, 'for lang in c; do\n' + dispatch + 'done\n'),
                     ALL.replace('done\nfor lang in c', dispatch + 'done\nfor lang in c'),
                     ALL.replace(dispatch, '').replace('for lang in c; do',
                         'for lang in c; do\n' + dispatch)):
            with self.subTest(text=text), self.assertRaises(gn.NeutralityError):
                gn._shell_recipe(text, 'tools/regen-all.sh')

    def test_code_delta_includes_mode_and_kind_with_equal_bytes(self):
        path = self.write('handwritten', 'same-bytes')
        base = self.commit()
        for change in ('mode', 'symlink'):
            with self.subTest(change=change):
                if change == 'mode':
                    path.chmod(0o755)
                else:
                    path.unlink()
                    path.symlink_to('same-bytes')
                delta = gn._code_delta(self.repo, base, gn._working_files(self.repo, []))
                self.assertEqual([c['path'] for c in delta], ['handwritten'])
                self.assertEqual(delta[0]['before_sha256'], delta[0]['after_sha256'])
        self.git('add', 'handwritten')
        self.git('-c', 'user.name=Test', '-c', 'user.email=t@example.invalid',
                 'commit', '-qm', 'symlink swap')
        self.assertEqual(gn._code_delta(self.repo, self.git('rev-parse', 'HEAD'),
                                       gn._working_files(self.repo, [])), [])

    def test_missing_child_and_additional_recipe_fail_closed(self):
        self.write('tools/regen-all.sh', ALL.replace('python rust', 'python'))
        self.assertRefused()
        self.write('tools/regen-all.sh', ALL)
        self.write('tools/regen-extra.sh', PYTHON)
        self.assertRefused()

    def test_unresolved_rust_invocation(self):
        self.write('tools/regen-rust.sh', RUST_SCRIPT.replace('regen_spec cmis', 'regen_spec other'))
        self.assertRefused()

    def test_source_missing_unreadable_symlink_and_invalid_revision(self):
        for source in ('tools/regen-all.sh', MAIN, RUST):
            path = self.repo / source
            original = path.read_bytes()
            path.unlink()
            self.assertRefused()
            path.symlink_to('/missing-source')
            self.assertRefused()
            path.unlink()
            path.write_bytes(b'\xff')
            with self.assertRaises(UnicodeError):
                self.scope()
            path.write_bytes(original)
        with self.assertRaises(gn.NeutralityError):
            self.scope('no-such-revision')

    def test_nested_emitter_sources_and_symlink_directories(self):
        nested = 'crates/openxcvr-codegen/src/emitters/c/deeper/writer.rs'
        self.write(nested, 'std::fs::write(out.join("new-name.bin"), code)?;')
        scope = self.scope()
        self.assertIn(nested, [s['path'] for s in scope['scope_sources']])
        base = self.commit()
        self.assertEqual(scope, self.scope(base))
        self.write(nested, 'std::fs::write(external, code)?;')
        self.assertRefused()
        shutil.rmtree(self.repo / 'crates/openxcvr-codegen/src/emitters/c')
        (self.repo / 'crates/openxcvr-codegen/src/emitters/c').symlink_to('/missing-emitter')
        self.assertRefused()
        base = self.commit()
        with self.assertRaises(gn.NeutralityError):
            self.scope(base)

    def test_emitter_new_filename_and_nested_destinations(self):
        self.write(MAIN, MAIN_SOURCE + '\nstd::fs::write(out.join("extra").join("new-name.bin"), code)?;')
        scope = self.scope()
        self.assertTrue(any('new-name.bin' in d['declaration']
                            for d in scope['emitter_destinations']))
        self.write('bindings/c/generated/extra/new-name.bin', 'new output')
        self.assertEqual(len(self.manifest()['manifest']), 1)

    def test_emitter_unresolved_escaping_and_new_writer_fail_closed(self):
        for addition in (
            'std::fs::write(PathBuf::from("elsewhere"), code)?;',
            'std::fs::write(out.join("../escape"), code)?;',
            'std::fs::write(out.join("/absolute"), code)?;',
            'std::fs::write(out.join(dynamic_path), code)?;',
            'std::fs::write(out.join("okay").with_extension("rs"), code)?;',
            'std::fs::write(out.join("unterminated", code)?;',
            'std::fs::copy(source, out.join("new"))?;',
            'std::fs::write(alias.join("new"), code)?;',
            'File::create(out.join("new"))?;',
            'use std::fs::write; write(out.join("new"), code)?;',
            'out = external; std::fs::write(out.join("new"), code)?;',
            'out.push("../escape"); std::fs::write(out.join("new"), code)?;',
        ):
            with self.subTest(addition=addition):
                self.write(MAIN, MAIN_SOURCE + addition)
                self.assertRefused()
        self.write(MAIN, MAIN_SOURCE)
        self.write('crates/openxcvr-codegen/src/emitters/c/new.rs',
                   'std::fs::write(external, code)?;')
        self.assertRefused()

    def test_emitter_changed_variable_bindings_fail_closed(self):
        for text in (
            RUST_SOURCE.replace('out_dir.join(format!("v{}", rev.dirname()))', 'elsewhere'),
            RUST_SOURCE.replace('let dir = out_dir.join(format!("v{}", rev.dirname()));', ''),
            RUST_SOURCE.replace('rev.dirname()', 'unknown_root()'),
        ):
            with self.subTest(text=text):
                self.write(RUST, text)
                self.assertRefused()
        self.write(RUST, RUST_SOURCE)
        self.write(MAIN, MAIN_SOURCE.replace('out.join(format!("{}.rs", schema.id))', 'outside'))
        self.assertRefused()

    def test_nested_function_cannot_truncate_emitter_audit(self):
        text = '''fn emit(out: &Path) {
let mut out = out.to_path_buf();
out = PathBuf::from("/tmp/x");
fn helper() {}
fs::write(out.join("escape"), code);
}'''
        with self.assertRaises(gn.NeutralityError):
            gn._audit_emitter(text, MAIN)

    def test_all_output_binding_patterns_fail_closed(self):
        patterns = (
            'for out in [PathBuf::from("/tmp/x")] { WRITE }',
            'for (out_dir, _) in paths { WRITE }',
            'if let Some(out) = external { WRITE }',
            'while let Some(out) = external { WRITE }',
            'match external { Some(out) => { WRITE }, _ => {} }',
            'match external { Root { out } => { WRITE }, _ => {} }',
            'let f = |out| { WRITE };',
            'let f = |out: &Path| { WRITE };',
            'let (out, _) = external; WRITE',
            'let f = |dir| { fs::write(dir.join("escape"), code); };',
            'for out_file in paths { fs::write(&out_file, code); }',
        )
        for pattern in patterns:
            with self.subTest(pattern=pattern), self.assertRaises(gn.NeutralityError):
                gn._audit_emitter('fn emit(out: &Path) { '
                    + pattern.replace('WRITE', 'fs::write(out.join("escape"), code);')
                    + ' }', MAIN)
        with self.assertRaises(gn.NeutralityError):
            gn._audit_emitter('fn emit(out: &Path, dir: &Path) { '
                             'let dir = out.join("safe"); fs::write(dir.join("x"), code); }', MAIN)

    def test_mutable_output_roots_fail_closed(self):
        protected = ('out', 'out_dir', 'dir', 'sub', 'out_file', 'header_rel', 'file_stem')
        cases = [
            'fn emit(mut out: PathBuf) { out.as_mut_os_string().push("/../../x"); WRITE }',
            'fn emit(mut out: PathBuf) { let mut evil = PathBuf::from("/tmp"); '
            'std::mem::swap(&mut out, &mut evil); WRITE }',
        ]
        for name in protected:
            cases.extend((
                f'fn emit(out: &Path, mut {name}: PathBuf) {{ WRITE }}',
                f'fn emit(out: &Path) {{ let mut {name} = external; WRITE }}',
                f'fn emit(out: &Path) {{ mutate(&mut {name}); WRITE }}',
                f'fn emit(out: &Path) {{ *(&mut {name}) = external; WRITE }}',
                f'fn emit(out: &Path) {{ {name}.as_mut_os_string(); WRITE }}',
                f'fn emit(out: &Path) {{ {name}.as_mut_path(); WRITE }}',
            ))
        for prefix in ('std::mem', 'mem'):
            for operation in ('swap', 'replace', 'take'):
                cases.append(f'fn emit(out: &Path) {{ {prefix}::{operation}(external); WRITE }}')
        for parameter in ('out: PathBuf', 'out: &mut Path', 'out_dir: PathBuf'):
            root = parameter.split(':')[0]
            cases.append(f'fn emit({parameter}) {{ fs::write({root}.join("a.rs"), code); }}')
        for text in cases:
            with self.subTest(text=text), self.assertRaises(gn.NeutralityError):
                gn._audit_emitter(text.replace('WRITE', 'fs::write(out.join("a.rs"), code);'), MAIN)
        for root in ('out', 'out_dir'):
            text = f'fn emit({root} : & Path) {{ fs::write({root}.join("a.rs"), code); }}'
            self.assertTrue(gn._audit_emitter(text, MAIN))

    def test_sub_has_only_literal_loop_binding(self):
        safe = '''fn emit(out: &Path) {
for sub in ["enums", "pages"] { fs::write(out.join(sub), code); }
}'''
        self.assertTrue(gn._audit_emitter(safe, MAIN))
        for binding in ('let sub = "../../x";', 'sub = "../../x";', 'for sub in paths {',
                        'let f = |sub| {', 'if let Some(sub) = path {',
                        'match path { Root { sub } => {'):
            with self.subTest(binding=binding), self.assertRaises(gn.NeutralityError):
                gn._audit_emitter(safe.replace('fs::write', binding + ' fs::write'), MAIN)

    def test_other_write_channels_fail_closed_even_without_fs_calls(self):
        for channel in ('std::process::Command::new("sh")', 'Command::new("sh")',
                        'process::Command::new("sh")', 'use std::process::Command;',
                        'std::net::TcpStream::connect(addr)',
                        'use std::os::unix::net::UnixStream;',
                        'libc::write(fd, bytes, len)', 'unsafe { external_write(); }'):
            with self.subTest(channel=channel), self.assertRaises(gn.NeutralityError):
                gn._audit_emitter(channel, MAIN)

    def test_emitter_language_or_missing_write_fail_closed(self):
        for text in (MAIN_SOURCE.replace('"c")', '"go")'),
                     'if !matches!(lang.as_str(), "rust" | "python" | "c") {}'):
            with self.subTest(text=text):
                self.write(MAIN, text)
                self.assertRefused()

    def test_cli_all_unimplemented_modes_refuse_without_mutation(self):
        context = self.write('caller.json', '{"schema_version":1,"mode":"task"}')
        before = self.git('status', '--porcelain', '--untracked-files=all')
        for task in ('10', '19'):
            for mode in ('task', 'finish', 'unknown'):
                context.write_text(json.dumps({'schema_version': 1, 'mode': mode}))
                for exercise in ([], ['--exercise-nine']):
                    result = subprocess.run([sys.executable, str(HELPER), 'verify',
                                             '--task', task, '--context', str(context), *exercise],
                                            cwd=self.repo, capture_output=True, text=True)
                    self.assertEqual(result.returncode, 2)
                    self.assertIn('ERROR:', result.stderr)
                    self.assertEqual(result.stdout, '')
        self.assertEqual(before, self.git('status', '--porcelain', '--untracked-files=all'))

    def test_cli_required_arguments_and_choices(self):
        for args in ([], ['verify'], ['verify', '--task', '10'],
                     ['verify', '--task', '24', '--context', '/missing']):
            result = subprocess.run([sys.executable, str(HELPER), *args],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
        help_result = subprocess.run([sys.executable, str(HELPER), 'verify', '--help'],
                                     capture_output=True, text=True)
        self.assertEqual(help_result.returncode, 0)
        self.assertIn('--exercise-nine', help_result.stdout)


class ContextTaskTests(unittest.TestCase):
    git = ScopeTests.git
    write = ScopeTests.write
    commit = ScopeTests.commit
    scope = ScopeTests.scope

    def setUp(self):
        ScopeTests.setUp(self)
        self.outer = tempfile.TemporaryDirectory(prefix='neutrality-parent-')
        self.addCleanup(self.outer.cleanup)
        self.evidence_root = Path(self.outer.name)
        self.helper = self.write('tools/check-generated-neutrality.py', HELPER.read_text())
        self.test = self.write('tools/test_check_generated_neutrality.py',
                              'import unittest\nclass Qualification(unittest.TestCase):\n'
                              '    def test_input(self):\n'
                              '        self.assertEqual(2 + 2, 4)\n'
                              'unittest.main()\n')
        # C11: executed recipes are fail-fast, so `$spec` is bound by the
        # same spec loop the real recipes use rather than left unset.
        for path in ('tools/regen-all.sh', 'tools/regen-python.sh', 'tools/regen-rust.sh'):
            script = self.repo / path
            script.write_text(executed(script.read_text()))
            script.chmod(0o755)
        if getattr(self, 'nine_fixture', False):
            self.configure_nine()
        if getattr(self, 'head_move_fixture', False):
            for name in self.head_move_committed:
                self.write(name, 'committed ' + name + '\n')
        self.output = self.write(ROOT + '/tracked.py', 'generated bytes\n')
        self.write('bindings/python/README.md', 'dirty handwritten baseline\n')
        self.base = self.commit()
        self.fake_bin = self.evidence_root / 'fake-bin'
        self.fake_bin.mkdir()
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('#!/bin/sh\n' + self.generation_script()
                         + 'printf "fake generation\\n"\n')
        cargo.chmod(0o755)
        self.env = patch.dict(os.environ, {'PATH': str(self.fake_bin) + os.pathsep + os.environ['PATH']})
        self.env.start()
        self.addCleanup(self.env.stop)
        self.build_admission()

    def build_admission(self, repo=None):
        """Synthetic parent evidence bound to the caller checkout named.

        Extracted from setUp so a second worktree root can obtain genuinely
        matching admission from this same harness instead of bypassing the
        caller-repo requirement of admit_context. The qualification pins are
        the named checkout's own committed helper and test files, and the
        qualification command runs with that checkout as its cwd.
        """
        repo = self.repo if repo is None else repo
        self.helper = repo / 'tools/check-generated-neutrality.py'
        self.test = repo / 'tools/test_check_generated_neutrality.py'
        snapshot = dict(run_id='synthetic-run', wave=0, status='pending', key='key-0', attempt=1,
                        wave_token='token-0', payload=[dict(id=10, files=['tools/', 'bindings/python/README.md'])],
                        review_context=dict(base_sha=self.base,
                        tasks=[dict(task_id=10, repo=str(repo))]))
        native = self.put('native.json', snapshot)
        snapshot_path = self.put('snapshot.json', snapshot)
        self.dispatch = dict(native_dispatch_path=str(native), dispatch_snapshot_path=str(snapshot_path),
                             dispatch_snapshot_sha256=gn.sha256(snapshot_path.read_bytes()))
        self.scope_at_base = gn.derive_scope(repo, self.base)
        baseline = gn.git_manifest(repo, self.base, self.scope_at_base['output_rules'])
        envelope = dict(schema_version=1, run_id='synthetic-run', repo=str(repo), task_id='10',
                        wave=0, base_sha=self.base, **self.dispatch,
                        scope_sources=self.scope_at_base['scope_sources'],
                        output_rules=self.scope_at_base['output_rules'],
                        before_manifest=baseline['manifest'], before_manifest_sha256=baseline['manifest_sha256'])
        envelope_path = self.put('prelaunch.json', envelope)
        result = subprocess.run(['python3', 'tools/test_check_generated_neutrality.py'], cwd=repo,
                                capture_output=True)
        self.assertEqual(result.returncode, 0)
        stdout = self.put('qualification.stdout', result.stdout)
        stderr = self.put('qualification.stderr', result.stderr)
        command = dict(argv=['python3', 'tools/test_check_generated_neutrality.py'], cwd=str(repo),
                       exit=result.returncode, stdout_path=str(stdout), stdout_sha256=gn.sha256(result.stdout),
                       stderr_path=str(stderr), stderr_sha256=gn.sha256(result.stderr))
        identity = dict(base_sha=self.base, helper_sha256=gn.sha256(self.helper.read_bytes()),
                        test_sha256=gn.sha256(self.test.read_bytes()),
                        scope_sources=gn.derive_scope(repo)['scope_sources'])
        qualified = dict(helper_path=str(self.helper), helper_sha256=identity['helper_sha256'],
                         test_path=str(self.test), test_sha256=identity['test_sha256'], source_identity=identity)
        receipt_path = self.put('qualification.json', dict(schema_version=1, run_id='synthetic-run',
                                repo=str(repo), task_id='10', commands=[command], **qualified))
        reference = dict(**qualified, qualification_receipt_path=str(receipt_path),
                         qualification_receipt_sha256=gn.sha256(receipt_path.read_bytes()))
        self.context = dict(schema_version=1, mode='task', run_id='synthetic-run', repo=str(repo),
                            evidence_root=str(self.evidence_root), dispatches={'10': str(native)},
                            prelaunch={'10': str(envelope_path)}, dispatch_evidence={'10': self.dispatch},
                            qualifications={'10': reference})
        self.context_path = self.put('context.json', self.context)

    def configure_nine(self):
        scripts = {
            'rust': RUST_SCRIPT.replace('synthetic-crate', 'openxcvr-sff8472'),
            'python': PYTHON,
        }
        roots = {
            'typescript': 'bindings/typescript/generated',
            'java': 'bindings/java/src/main/java/com/openxcvr/generated',
            'csharp': 'bindings/csharp/generated',
            'openapi': 'bindings/openapi/generated',
            'go': 'bindings/go/generated',
            'php': 'bindings/php/src/Generated',
        }
        for lang, root in roots.items():
            scripts[lang] = PYTHON.replace(ROOT, root).replace('--lang python', '--lang ' + lang)
        for lang, text in scripts.items():
            self.write('tools/regen-' + lang + '.sh', executed(text)).chmod(0o755)
        self.write('tools/regen-all.sh', executed(ALL.replace('python rust', ' '.join(scripts)))).chmod(0o755)
        self.write(MAIN, MAIN_SOURCE.replace('"rust" | "python" | "c"',
            ' | '.join('"' + lang + '"' for lang in [*scripts, 'c'])))
        for index, path in enumerate(gn.NINE_PATHS):
            self.write(path, f'original language {index}\n')

    def generation_script(self):
        script = 'printf "generated bytes\\n" > "' + str(self.output) + '"\n'
        if getattr(self, 'nine_fixture', False):
            for index, path in enumerate(gn.NINE_PATHS):
                script += f'printf "original language {index}\\n" > "{self.repo / path}"\n'
        return script

    def put(self, name, value):
        path = self.evidence_root / name
        path.write_bytes(value if isinstance(value, bytes) else gn._encoded(value))
        return path

    def admit(self, context=None, task='10'):
        if context is not None:
            self.put('context.json', context)
        return gn.admit_context(str(self.context_path), task, self.repo, self.helper, self.test)

    def cli(self, *args):
        return subprocess.run([sys.executable, str(self.helper), 'verify', '--task', '10',
                               '--context', str(self.context_path), *args], cwd=self.repo,
                              capture_output=True, text=True)

    def pins(self):
        return {p.relative_to(self.evidence_root).as_posix(): p.read_bytes()
                for p in self.evidence_root.rglob('*') if p.is_file()}

    def test_executable_echo_recipe_refused_before_generation(self):
        # R2 through the real admission seam: an execution-bearing echo is
        # refused before a generator can run, so its owned marker never exists.
        script = self.repo / 'tools/regen-all.sh'
        original = script.read_text()
        marker = self.evidence_root / 'echo-marker'
        for kind, line in (('dollar', f'echo "$(touch {marker})"'),
                           ('backtick', 'echo "`touch ' + str(marker) + '`"'),
                           ('array', 'echo "${SPECS[@]}"'),
                           ('redirect', 'echo "progress" > ' + str(marker))):
            with self.subTest(kind=kind):
                script.write_text(original + line + '\n')
                try:
                    result = self.cli()
                    self.assertEqual(result.returncode, 2)
                    self.assertIn('unparsed regeneration source', result.stderr)
                    self.assertFalse(marker.exists())
                    self.assertFalse((self.evidence_root / 'task-10').exists())
                finally:
                    script.write_text(original)

    def test_recipe_without_fail_fast_prologue_refused_before_generation(self):
        # C11: without `set -euo pipefail` a failing generator can be followed
        # by a later successful command, so Bash exits 0 and the unchanged
        # generated files satisfy both manifest comparisons. A recipe that does
        # not set it before its first command refuses at admission.
        script = self.repo / 'tools/regen-python.sh'
        script.write_text(script.read_text().replace('set -euo pipefail\n', '')
                          + 'echo "Done."\n')
        invocations = self.evidence_root / 'cargo-invocations'
        (self.fake_bin / 'cargo').write_text(
            '#!/bin/sh\nprintf "%s\\n" "$*" >> "' + str(invocations) + '"\n'
            'case "$*" in *"--lang python"*) exit 3 ;; esac\n'
            + self.generation_script() + 'printf "fake generation\\n"\n')
        try:
            self.build_admission()  # binds the edited recipe where it is admissible
        except gn.NeutralityError:
            pass
        result = self.cli()
        self.assertEqual((result.returncode, list(self.evidence_root.glob('task-10/*/receipt.json'))),
                         (2, []), result.stdout + result.stderr)
        self.assertIn('set -euo pipefail', result.stderr)
        self.assertFalse(invocations.exists(), 'generator ran before the recipe was refused')
        self.assertFalse((self.evidence_root / 'task-10').exists())
        # Present but late, or weaker, is not fail-fast either.
        body = PYTHON.replace('set -euo pipefail\n', '')
        for text in (body + 'set -euo pipefail\n', 'set -e\n' + body, 'set -eu\n' + body):
            with self.subTest(text=text.splitlines()[0]), \
                    self.assertRaisesRegex(gn.NeutralityError, 'set -euo pipefail'):
                gn._shell_recipe(text, 'tools/regen-python.sh')
        gn._shell_recipe('#!/usr/bin/env bash\n# comment\n\n' + PYTHON, 'tools/regen-python.sh')

    def test_inert_progress_echo_admitted_and_generates(self):
        # R2 positive control: an actual progress message with literal text and
        # simple variable references stays admissible and still generates.
        script = self.repo / 'tools/regen-all.sh'
        script.write_text(script.read_text() + 'echo "\u2192 $spec ($lang) \u2192 $OUT"\n')
        # The baseline output already holds the generated bytes, so an exit
        # status alone cannot show generation ran: this test's fake Cargo logs
        # one line per invocation and the recipe's generators must appear.
        invocations = self.evidence_root / 'cargo-invocations'
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> "' + str(invocations) + '"\n'
                         + self.generation_script() + 'printf "fake generation\\n"\n')
        self.build_admission()
        receipt = gn.verify_task(self.admit())
        self.assertEqual([c['exit'] for c in receipt['commands']], [0, 0])
        lines = invocations.read_text().splitlines() if invocations.exists() else []
        generator = 'run -q -p openxcvr-codegen -- generate '
        self.assertTrue(all(line.startswith(generator) for line in lines), lines)
        langs = [line.split('--lang ', 1)[1].split()[0] for line in lines]
        self.assertEqual(langs, ['python', 'rust', 'c'] * 2, 'recipe generators did not run in both passes')

    def test_caller_bash_env_hook_cannot_precede_the_recipe(self):
        # C4: `BASH_ENV` is expanded by the non-interactive `bash
        # tools/regen-all.sh` launch, which would run an unqualified caller file
        # before the audited recipe and let it act outside the snapshot.
        hooks = tempfile.TemporaryDirectory(prefix='neutrality-bash-env-')
        self.addCleanup(hooks.cleanup)
        marker = Path(hooks.name) / 'hook-ran'
        hook = Path(hooks.name) / 'hook.sh'
        hook.write_text(': > "' + str(marker) + '"\n')
        with patch.dict(os.environ, {'BASH_ENV': str(hook)}):
            receipt = gn.verify_task(self.admit())
        self.assertEqual([c['exit'] for c in receipt['commands']], [0, 0])
        self.assertFalse(marker.exists())

    def test_caller_exported_cargo_function_never_runs(self):
        marker = self.evidence_root / 'exported-cargo-ran'
        function = '() { : > "' + str(marker) + '"; command cargo "$@"; }'
        with patch.dict(os.environ, {'BASH_FUNC_cargo%%': function}):
            receipt = gn.verify_task(self.admit())
        self.assertEqual([c['exit'] for c in receipt['commands']], [0, 0])
        self.assertFalse(marker.exists(), 'caller-exported cargo function ran')

    def test_toolchain_selection_reaches_probe_and_generation_unchanged(self):
        observed = self.evidence_root / 'child-environment'
        cargo = self.fake_bin / 'cargo'
        target = self.repo / 'target'
        self.write('Cargo.toml', '[workspace]\n')
        cargo.write_text('#!/bin/sh\n'
            + 'printf "%s|%s|%s|%s\\n" "$RUSTUP_TOOLCHAIN" "$CARGO_HOME" '
              '"$RUSTUP_HOME" "${NEUTRALITY_UNLISTED-unset}" >> "' + str(observed) + '"\n'
            + 'if [ "$1" = metadata ]; then\nprintf \'%s\' \''
            + json.dumps({'target_directory': str(target)}) + "'\nexit 0\nfi\n"
            + self.generation_script())
        self.build_admission()
        selection = {'RUSTUP_TOOLCHAIN': 'synthetic-pinned-toolchain',
                     'CARGO_HOME': str(self.evidence_root / 'relocated-cargo'),
                     'RUSTUP_HOME': str(self.evidence_root / 'relocated-rustup'),
                     'NEUTRALITY_UNLISTED': 'must-not-reach-child'}
        with patch.dict(os.environ, selection):
            receipt = gn.verify_task(self.admit())
        self.assertEqual([c['exit'] for c in receipt['commands']], [0, 0])
        expected = '|'.join(selection[k] for k in
                            ('RUSTUP_TOOLCHAIN', 'CARGO_HOME', 'RUSTUP_HOME')) + '|unset'
        lines = observed.read_text().splitlines()
        self.assertGreaterEqual(len(lines), 3)  # metadata and both regenerations
        self.assertEqual(set(lines), {expected}, 'unlisted variable reached child')

    def test_launch_environment_allowlist_and_override_diagnostics(self):
        allowed = {'PATH', 'HOME', 'TMPDIR', 'LANG', 'LC_ALL', 'LC_CTYPE',
                   'CARGO_HOME', 'RUSTUP_HOME', 'RUSTUP_TOOLCHAIN'}
        dropped = {'BASH_ENV', 'ENV', 'BASH_FUNC_any_name%%', 'CARGO_TARGET_DIR',
                   'CARGO_BUILD_TARGET_DIR', 'CC', 'CXX', 'AR', 'LD', 'CARGO_OTHER',
                   'NEUTRALITY_UNLISTED', 'USER', 'LOGNAME'}
        caller = {name: 'value-' + name for name in allowed | dropped}
        with patch.dict(os.environ, caller, clear=True):
            self.assertEqual(gn._cargo_environment(), {k: caller[k] for k in allowed})
        with patch.dict(os.environ, {}, clear=True):
            self.assertEqual(gn._cargo_environment(), {})
        overrides = ('RUSTC', 'RUSTDOC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER',
                     'RUSTFLAGS', 'RUSTDOCFLAGS', 'CARGO_ENCODED_RUSTFLAGS',
                     'CARGO_ENCODED_RUSTDOCFLAGS', 'CARGO_BUILD_RUSTC_WRAPPER',
                     'CARGO_BUILD_FUTURE_OVERRIDE', 'CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_LINKER',
                     'CARGO_TARGET_CUSTOM_RUNNER', 'CARGO_TARGET_CUSTOM_RUSTFLAGS')
        for name in overrides:
            with self.subTest(name=name), patch.dict(os.environ, {name: ''}, clear=True):
                self.assertEqual(gn._launch_overrides(os.environ), [name])
                for guard in (gn._refuse_launch_overrides, gn._cargo_environment):
                    with self.assertRaisesRegex(gn.NeutralityError,
                                                name + '; unset it for the helper run'):
                        guard()

    def test_recipe_edit_at_each_regeneration_boundary_refuses(self):
        # R3: the qualified rule sources are revalidated before every actual
        # regeneration and after generation, so a boundary edit cannot yield a
        # receipt carrying stale source hashes.
        script = self.repo / 'tools/regen-all.sh'
        original = script.read_text()
        for boundary in ('regen-0', 'regen-1'):
            with self.subTest(boundary=boundary):
                if (self.evidence_root / 'task-10').exists():
                    shutil.rmtree(self.evidence_root / 'task-10')
                script.write_text(original)
                admitted = self.admit()
                real = gn._regenerate

                def seam(a, directory, label, exclusions, boundary=boundary):
                    if label == boundary:
                        script.write_text(original + 'echo "changed-qualified-script"\n')
                    return real(a, directory, label, exclusions)

                with patch.object(gn, '_regenerate', side_effect=seam):
                    with self.assertRaisesRegex(gn.NeutralityError,
                                                'current rule sources changed'):
                        gn.verify_task(admitted)
                self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
                script.write_text(original)

    def test_helper_or_test_bytes_changed_at_boundary_refuse(self):
        for role in ('helper', 'test'):
            with self.subTest(role=role):
                if (self.evidence_root / 'task-10').exists():
                    shutil.rmtree(self.evidence_root / 'task-10')
                path = getattr(self, role)
                original = path.read_bytes()
                admitted = self.admit()
                real = gn._regenerate

                def seam(a, directory, label, exclusions, path=path, original=original):
                    if label == 'regen-0':
                        path.write_bytes(original + b'# boundary drift\n')
                    return real(a, directory, label, exclusions)

                try:
                    with patch.object(gn, '_regenerate', side_effect=seam):
                        with self.assertRaisesRegex(gn.NeutralityError,
                                                    f'current {role} changed'):
                            gn.verify_task(admitted)
                    self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
                finally:
                    path.write_bytes(original)

    def test_post_execution_identity_drift_refuses_after_recovery(self):
        # R3: a mismatch detected after generation must not preempt recovery and
        # must not emit a receipt.
        dirty = self.repo / 'bindings/python/README.md'
        original = dirty.read_bytes()
        assert original != b'clobbered'
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('\n'.join(['#!/bin/sh', self.generation_script(),
            f'printf "clobbered" > "{dirty}"',
            f'git -C "{self.repo}" add -- bindings/python/README.md',
            f'git -C "{self.repo}" -c "user.name=Test" -c "user.email=t@example.invalid"'
            ' commit -qm "mid-run commit"', 'exit 0']) + '\n')
        admitted = self.admit()
        with self.assertRaises(gn.NeutralityError) as failure:
            gn.verify_task(admitted)
        self.assertIn('verified HEAD moved', str(failure.exception))
        self.assertEqual(dirty.read_bytes(), original)
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))

    def test_generation_executable_mode_drift_refuses(self):
        # R5: bytes equal is not reproducibility; a regeneration that changes
        # the Git executable bit cannot receive a receipt.
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('#!/bin/sh\n' + self.generation_script()
                         + f'\nchmod 755 "{self.output}"\n')
        admitted = self.admit()
        with self.assertRaisesRegex(gn.NeutralityError, 'executable mode'):
            gn.verify_task(admitted)
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))

    def test_unchanged_and_non_executable_modes_stay_admitted(self):
        # R5 positive controls: no mode change, and permission differences Git
        # does not treat as executable (only the owner-execute bit decides).
        # 0o654 and 0o645 are the group-only and other-only execute controls:
        # they differ from the base permissions yet stay Git-non-executable.
        for mode in (0o644, 0o640, 0o600, 0o664, 0o654, 0o645):
            with self.subTest(mode=oct(mode)):
                if (self.evidence_root / 'task-10').exists():
                    shutil.rmtree(self.evidence_root / 'task-10')
                self.output.chmod(mode)
                receipt = gn.verify_task(self.admit())
                self.assertEqual([c['exit'] for c in receipt['commands']], [0, 0])
                self.assertFalse(gn._git_executable(self.output.stat().st_mode), oct(mode))

    def test_preexisting_task_executable_mode_drift_refuses_at_admission(self):
        # R5/A-R5-1: task/prelaunch admission is one of the four named
        # comparison points. Generated bytes equal to the dispatch base do not
        # excuse a differing Git executable mode: 0o750 sets the owner-execute
        # bit while the base git object is 100644, so a receipt would certify
        # neutrality for output that is not mode-neutral.
        self.output.chmod(0o750)
        try:
            with self.assertRaisesRegex(gn.NeutralityError, 'executable mode'):
                self.admit()
            self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
        finally:
            self.output.chmod(0o644)
        # The identical fixture at the base mode still receives its receipt.
        receipt = gn.verify_task(self.admit())
        self.assertEqual([c['exit'] for c in receipt['commands']], [0, 0])

    def test_preexisting_handwritten_mode_drift_is_recorded_in_the_delta(self):
        # The _code_delta owner-execute normalization is about hand-authored
        # paths, which R5 does not govern: a mode change there is recorded in
        # the byte-equal delta rather than refused. This preserves what the old
        # generated-path fixture asserted, which R5 now refuses.
        path = self.repo / 'bindings/python/README.md'
        path.chmod(0o750)
        receipt = gn.verify_task(self.admit())
        change = next(c for c in receipt['code_delta'] if c['path'] == 'bindings/python/README.md')
        self.assertEqual((change['before_mode'], change['after_mode']), (0o644, 0o755))
        self.assertEqual(change['before_sha256'], change['after_sha256'])

    def test_unexpected_git_entries_refuse_before_generation(self):
        # R6: an unadmitted .git entry is ambiguous. It refuses before any
        # generator runs, and its contents are never read or descended into.
        for kind in ('file', 'directory', 'symlink'):
            with self.subTest(kind=kind):
                target = self.repo / 'handwritten/.git'
                target.parent.mkdir(parents=True, exist_ok=True)
                if kind == 'file':
                    target.write_text('synthetic marker')
                elif kind == 'directory':
                    (target / 'payload').parent.mkdir(parents=True)
                    (target / 'payload').write_text('synthetic marker')
                else:
                    target.symlink_to(self.repo / '.git')
                try:
                    admitted = self.admit()
                    original_read = Path.read_bytes

                    def read(path):
                        if 'handwritten/.git' in str(path):
                            raise AssertionError('unexpected .git entry was read')
                        return original_read(path)

                    with patch.object(Path, 'read_bytes', read):
                        with self.assertRaisesRegex(gn.NeutralityError, 'unexpected .git entry'):
                            gn.verify_task(admitted)
                    self.assertFalse((self.evidence_root / 'task-10').exists())
                    self.assertTrue(target.is_symlink() or target.exists())
                finally:
                    shutil.rmtree(target, ignore_errors=True)
                    if target.is_symlink() or target.exists():
                        target.unlink()

    def test_generator_created_unexpected_git_entry_fails_without_traversal(self):
        target = self.repo / 'handwritten/.git'
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('#!/bin/sh\n' + self.generation_script()
                         + f'\nmkdir -p "{target.parent}"\nprintf "synthetic marker" > "{target}"\n')
        admitted = self.admit()
        original_read = Path.read_bytes

        def read(path):
            if 'handwritten/.git' in str(path):
                raise AssertionError('unexpected .git entry was read')
            return original_read(path)

        with patch.object(Path, 'read_bytes', read):
            with self.assertRaisesRegex(gn.NeutralityError, 'unexpected .git entry'):
                gn.verify_task(admitted)
        self.assertEqual(target.read_text(), 'synthetic marker')
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))

    def test_actual_metadata_stays_out_of_source_snapshots(self):
        self.assertFalse([p for p in gn._working_files(self.repo, []) if '.git' in Path(p).parts])
        linked = self.evidence_root / 'linked'
        self.git('worktree', 'add', '--detach', str(linked), self.base)
        self.assertFalse([p for p in gn._working_files(linked, []) if '.git' in Path(p).parts])

    def test_separate_git_dir_inside_worktree_is_metadata_not_source(self):
        # C6: Git's reported admin/common location is excluded by its reported
        # path, not its basename. `--separate-git-dir` on an existing repo moves
        # the metadata to `admin/` and leaves a `.git` gitfile pointing at it.
        self.git('init', '-q', '--separate-git-dir=' + str(self.repo / 'admin'), str(self.repo))
        self.assertEqual(gn._metadata_entries(self.repo), {'admin', '.git'})
        admin = self.repo / 'admin'
        nested = self.repo / 'handwritten/.git'
        original_read, original_readlink = Path.read_bytes, os.readlink

        def forbidden(path):
            path = Path(path)
            if path.is_relative_to(admin) or path.is_relative_to(nested):
                raise AssertionError('Git metadata was read as source: ' + str(path))

        def read(path):
            forbidden(path)
            return original_read(path)

        def readlink(path, *args, **kwargs):
            forbidden(path)
            return original_readlink(path, *args, **kwargs)

        with patch.object(Path, 'read_bytes', read), patch.object(os, 'readlink', readlink):
            working = gn._working_files(self.repo, [])
        self.assertFalse([p for p in working if p == 'admin' or p.startswith('admin/')])
        # A linked worktree of that repository still snapshots without metadata.
        linked = self.evidence_root / 'linked-separate'
        self.git('worktree', 'add', '--detach', str(linked), self.base)
        self.assertFalse([p for p in gn._working_files(linked, [])
                          if '.git' in Path(p).parts or p.startswith('admin/')])
        # An unexpected nested `.git` still refuses without being read.
        self.write('handwritten/.git', 'synthetic marker')
        with patch.object(Path, 'read_bytes', read), patch.object(os, 'readlink', readlink), \
             self.assertRaisesRegex(gn.NeutralityError, 'unexpected .git entry'):
            gn._working_files(self.repo, [])
        self.assertEqual(nested.read_text(), 'synthetic marker')

    def test_recovery_state_under_separate_git_dir_is_not_generated_dirt(self):
        # C6: the private recovery namespace lives under the common dir
        # (`admin/`); pending state written there between the before/after
        # snapshots is neither generated dirt nor part of the code delta.
        self.git('init', '-q', '--separate-git-dir=' + str(self.repo / 'admin'), str(self.repo))
        receipt = gn.verify_task(self.admit())
        self.assertEqual([c['exit'] for c in receipt['commands']], [0, 0])
        for state in receipt['working_state']:
            self.assertEqual(state['out_of_domain'], [])
            self.assertFalse([p for p in (*state['before'], *state['after'])
                              if p.startswith('admin/')])
        self.assertFalse([c for c in receipt['code_delta'] if c['path'].startswith('admin/')])
        self.assertTrue((self.repo / 'admin/openxcvr-neutrality-recovery').is_dir())

    def declare_cargo_target(self, target, declaration=None):
        """Untracked Cargo.toml; optional committed `build.target-dir`; fake Cargo
        reporting `target` and, if generation ever runs, writing into it."""
        self.write('Cargo.toml', '[workspace]\n')
        if declaration is not None:
            self.write('.cargo/config.toml', '[build]\ntarget-dir = ' + json.dumps(declaration) + '\n')
            self.git('add', '.cargo/config.toml')
            if self.git('diff', '--cached', '--name-only'):
                self.git('-c', 'user.name=Test', '-c', 'user.email=t@example.invalid',
                         'commit', '-qm', 'declared Cargo target')
        marker = self.evidence_root / 'generation-ran'
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('#!/bin/sh\nif [ "$1" = metadata ]; then\n'
            + "printf '%s' '" + json.dumps({'target_directory': str(target)}) + "'\nexit 0\nfi\n"
            + f'printf ran > "{marker}"\n' + self.generation_script()
            + f'mkdir -p "{target}/debug"\nprintf build > "{target}/debug/artifact"\n')
        return marker

    def assertMetadataTargetRefused(self, target, metadata, marker):
        """Refused before any generation, naming the overlapping metadata root."""
        with self.assertRaisesRegex(gn.NeutralityError,
                                    'Cargo target exclusion overlaps Git metadata: ' + metadata):
            gn.verify_task(self.admit())
        self.assertFalse(marker.exists())
        self.assertFalse((target / 'debug').exists())
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))

    def test_declared_cargo_target_equal_to_separate_git_dir_refuses(self):
        # C6b: C6 skips Git's reported metadata root by path, so a Cargo target
        # that is that root would let generation write into Git's administration
        # directory while both snapshots stay identical.
        self.git('init', '-q', '--separate-git-dir=' + str(self.repo / 'admin'), str(self.repo))
        target = self.repo / 'admin'
        marker = self.declare_cargo_target(target, 'admin')
        self.assertMetadataTargetRefused(target, 'admin', marker)

    def test_default_cargo_target_equal_to_separate_git_dir_refuses(self):
        self.git('init', '-q', '--separate-git-dir=' + str(self.repo / 'target'), str(self.repo))
        target = self.repo / 'target'
        marker = self.declare_cargo_target(target)
        self.assertMetadataTargetRefused(target, 'target', marker)

    def test_cargo_target_overlapping_separate_git_dir_refuses_in_every_spelling(self):
        # Equal, descendant, absolute, non-normalized and symlinked declarations
        # all resolve onto the metadata root and refuse; a disjoint declared
        # target beside the separate Git directory stays accepted.
        self.git('init', '-q', '--separate-git-dir=' + str(self.repo / 'admin'), str(self.repo))
        (self.repo / 'alias').symlink_to('admin')
        for declaration, target in (('admin/build', self.repo / 'admin/build'),
                                    (str(self.repo / 'admin'), self.repo / 'admin'),
                                    ('handwritten/../admin', self.repo / 'admin'),
                                    ('alias', self.repo / 'admin')):
            with self.subTest(declaration=declaration):
                marker = self.declare_cargo_target(target, declaration)
                self.assertMetadataTargetRefused(target, 'admin', marker)
        target = self.repo / 'cargo-out'
        self.declare_cargo_target(target, 'cargo-out')
        self.assertEqual(gn._build_exclusions(self.repo), [target])
        receipt = gn.verify_task(self.admit())
        self.assertEqual([c['exit'] for c in receipt['commands']], [0, 0])

    def test_cargo_target_containing_separate_git_dir_refuses(self):
        (self.repo / 'build').mkdir()
        self.git('init', '-q', '--separate-git-dir=' + str(self.repo / 'build/admin'), str(self.repo))
        target = self.repo / 'build'
        marker = self.declare_cargo_target(target, 'build')
        self.assertMetadataTargetRefused(target, 'build/admin', marker)

    def test_unadmitted_nested_repository_refuses(self):
        nested = self.repo / 'handwritten/nested'
        nested.mkdir(parents=True)
        subprocess.run(['git', '-C', str(nested), 'init', '-q'], check=True)
        (nested / 'source.txt').write_text('nested source')
        with self.assertRaisesRegex(gn.NeutralityError, 'unexpected .git entry'):
            gn._working_files(self.repo, [])

    def test_task_mode_passes_without_postcommit_binding_and_is_idempotent(self):
        pins = self.pins()
        self.write('bindings/python/README.md', 'handwritten task change\n')
        admitted = self.admit()
        self.assertNotIn('bindings', self.context)
        first = gn.verify_task(admitted)
        self.assertEqual(len(first['commands']), 2)
        self.assertEqual(first['before_manifest'], first['after_manifest'])
        for path, data in pins.items():
            self.assertEqual((self.evidence_root / path).read_bytes(), data)
        after = self.pins()
        second = gn.verify_task(self.admit())
        self.assertEqual(first, second)
        self.assertEqual(after, self.pins())
        self.assertEqual(self.cli().returncode, 0)
        self.assertEqual(self.pins(), after)
        self.assertEqual((self.repo / 'bindings/python/README.md').read_text(), 'handwritten task change\n')

    def test_replaced_base_commit_is_never_validated_under_its_own_sha(self):
        # C13: with refs/replace/<base>, rev-parse still names the base while
        # ls-tree/cat-file/show read the replacement, so a receipt naming the
        # base would bind another tree. Here the working tree matches only the
        # replacement: the original base's generated bytes differ.
        replaced = b'replacement generated bytes\n'
        self.output.write_bytes(replaced)
        self.commit()
        tree = self.git('rev-parse', 'HEAD^{tree}')
        self.git('update-ref', 'HEAD', self.base)
        replacement = self.git('-c', 'user.name=Test', '-c', 'user.email=t@example.invalid',
                               'commit-tree', tree, '-m', 'replacement for the recorded base')
        self.git('replace', self.base, replacement)
        self.assertEqual(self.git('rev-parse', self.base + '^{commit}'), self.base)
        original = subprocess.run(['git', '--no-replace-objects', '-C', str(self.repo), 'show',
                                   self.base + ':' + ROOT + '/tracked.py'],
                                  capture_output=True, check=True).stdout
        self.assertEqual(original, b'generated bytes\n')
        (self.fake_bin / 'cargo').write_text(
            '#!/bin/sh\nprintf "replacement generated bytes\\n" > "' + str(self.output) + '"\n')
        try:
            self.build_admission()
        except gn.NeutralityError:
            pass
        try:
            receipt = gn.verify_task(self.admit())
        except gn.NeutralityError as error:
            outcome = dict(refused=True, detail=str(error))
        else:
            hashes = {e['path']: e['sha256'] for e in receipt['before_manifest']}
            outcome = dict(refused=False, base=receipt['base_sha'],
                           before=hashes.get(ROOT + '/tracked.py'))
        # Either refusal (the tree is dirt against the original base), or a
        # receipt whose baseline is the original object.
        if outcome['refused']:
            self.assertIn('generated manifest', outcome['detail'])
        else:
            self.assertEqual(outcome, dict(refused=False, base=self.base,
                                           before=gn.sha256(original)))
        self.assertNotEqual(outcome.get('before'), gn.sha256(replaced))

    def test_every_git_child_disables_replacement_objects(self):
        # C13 audit: each git child the helper starts carries --no-replace-objects.
        real_run, argvs = gn.subprocess.run, []
        def run(argv, *args, **kwargs):
            if argv and argv[0] == 'git':
                argvs.append(list(argv))
            return real_run(argv, *args, **kwargs)
        with patch.object(gn.subprocess, 'run', side_effect=run):
            gn.verify_task(self.admit())
        self.assertTrue(argvs)
        self.assertEqual([a for a in argvs if a[:2] != ['git', '--no-replace-objects']], [])

    def test_context_field_test_rejects_oserror(self):
        case = ContextTaskTests('test_each_required_context_field_is_not_optional')
        case.context, case.dispatch = self.context, self.dispatch
        def admit(changed):
            if len(changed) == len(self.context) - 1:
                raise OSError('not a validation verdict')
            return self.admit(changed)
        with patch.object(case, 'admit', side_effect=admit):
            with self.assertRaises(OSError):
                case.test_each_required_context_field_is_not_optional()

    def test_each_required_context_field_is_not_optional(self):
        for name in self.context:
            changed = copy.deepcopy(self.context)
            del changed[name]
            with self.subTest(field=name), self.assertRaises(gn.NeutralityError):
                self.admit(changed)
        for map_name in ('dispatches', 'prelaunch', 'dispatch_evidence', 'qualifications'):
            changed = copy.deepcopy(self.context)
            changed[map_name] = {}
            with self.subTest(map=map_name), self.assertRaises(gn.NeutralityError):
                self.admit(changed)
        for name in self.dispatch:
            changed = copy.deepcopy(self.context)
            del changed['dispatch_evidence']['10'][name]
            with self.subTest(dispatch=name), self.assertRaises(gn.NeutralityError):
                self.admit(changed)
        for name in self.context['qualifications']['10']:
            changed = copy.deepcopy(self.context)
            del changed['qualifications']['10'][name]
            with self.subTest(qualification=name), self.assertRaises(gn.NeutralityError):
                self.admit(changed)

    def test_snapshot_digest_tampering_refused_before_regeneration(self):
        snapshot = self.evidence_root / 'snapshot.json'
        snapshot.write_bytes(snapshot.read_bytes() + b' ')
        with self.assertRaisesRegex(gn.NeutralityError, 'digest mismatch'):
            self.admit()
        self.assertFalse((self.evidence_root / 'task-10').exists())

    def test_task_absent_duplicate_wrong_repo_and_wrong_base(self):
        original = json.loads((self.evidence_root / 'snapshot.json').read_text())
        changes = [dict(tasks=[]), dict(tasks=original['review_context']['tasks'] * 2),
                   dict(tasks=[dict(task_id=10, repo='/wrong-repo')]), dict(base_sha='0' * 40)]
        for change in changes:
            with self.subTest(change=change):
                snapshot = copy.deepcopy(original)
                snapshot['review_context'].update(change)
                path = self.put('snapshot.json', snapshot)
                context = copy.deepcopy(self.context)
                context['dispatch_evidence']['10']['dispatch_snapshot_sha256'] = gn.sha256(path.read_bytes())
                with self.assertRaises(gn.NeutralityError):
                    self.admit(context)

    def test_modes_versions_wrong_run_repo_paths_and_snapshot_alias(self):
        for change in (dict(mode='unknown'), dict(schema_version=2), dict(schema_version=True),
                       dict(run_id='wrong-run'), dict(repo=str(self.evidence_root)), dict(run_id='')):
            with self.subTest(change=change), self.assertRaises(gn.NeutralityError):
                self.admit(dict(self.context, **change))
        changed = copy.deepcopy(self.context)
        changed['dispatch_evidence']['10']['dispatch_snapshot_path'] = changed['dispatches']['10']
        with self.assertRaisesRegex(gn.NeutralityError, 'aliases'):
            self.admit(changed)
        with self.assertRaises(gn.NeutralityError):
            gn.admit_context('relative.json', '10', self.repo, self.helper, self.test)
        (self.evidence_root / 'link-context.json').symlink_to(self.context_path)
        with self.assertRaises(gn.NeutralityError):
            gn.admit_context(str(self.evidence_root / 'link-context.json'), '10', self.repo, self.helper, self.test)

    def test_finish_requires_both_tasks_bindings_and_head(self):
        context = copy.deepcopy(self.context)
        context['mode'] = 'finish'
        with self.assertRaises(gn.NeutralityError):
            self.admit(context)
        for name in ('dispatches', 'prelaunch', 'dispatch_evidence', 'qualifications'):
            context[name]['19'] = context[name]['10']
        context['bindings'] = {'10': str(self.put('binding10.json', {})),
                               '19': str(self.put('binding19.json', {}))}
        context['finish_head'] = self.base
        with self.assertRaisesRegex(gn.NeutralityError, 'parent binding'):
            self.admit(context)
        context['finish_head'] = '0' * 40
        with self.assertRaises(gn.NeutralityError):
            self.admit(context)

    def test_stale_manifest_in_envelope_and_current_generated_dirt(self):
        envelope = json.loads((self.evidence_root / 'prelaunch.json').read_text())
        envelope['before_manifest'] = []
        self.put('prelaunch.json', envelope)
        with self.assertRaisesRegex(gn.NeutralityError, 'before_manifest'):
            self.admit()
        envelope['before_manifest'] = gn.git_manifest(self.repo, self.base, self.scope_at_base['output_rules'])['manifest']
        self.put('prelaunch.json', envelope)
        for path in (ROOT + '/tracked.py', ROOT + '/untracked.py', ROOT + '/hidden.ignored'):
            with self.subTest(path=path):
                self.write(path, 'wrong generated bytes')
                with self.assertRaisesRegex(gn.NeutralityError, 'generated manifest'):
                    self.admit()
                (self.repo / path).unlink()
                self.output.write_text('generated bytes\n')

    def test_qualification_digest_source_helper_test_exit_and_log_checks(self):
        original = copy.deepcopy(self.context)
        for role in ('helper', 'test', 'qualification_receipt'):
            changed = copy.deepcopy(original)
            changed['qualifications']['10'][role + '_sha256'] = '0' * 64
            with self.subTest(role=role), self.assertRaises(gn.NeutralityError):
                self.admit(changed)
        changed = copy.deepcopy(original)
        changed['qualifications']['10']['source_identity']['scope_sources'] = []
        with self.assertRaisesRegex(gn.NeutralityError, 'scope_sources'):
            self.admit(changed)
        qualified = json.loads((self.evidence_root / 'qualification.json').read_text())
        for alteration in (dict(task_id='19'), dict(run_id='different'), dict(commands=[])):
            changed_receipt = dict(qualified, **alteration)
            path = self.put('qualification.json', changed_receipt)
            changed = copy.deepcopy(original)
            changed['qualifications']['10']['qualification_receipt_sha256'] = gn.sha256(path.read_bytes())
            with self.subTest(alteration=alteration), self.assertRaises(gn.NeutralityError):
                self.admit(changed)
        qualified['commands'][0]['exit'] = 1
        path = self.put('qualification.json', qualified)
        changed = copy.deepcopy(original)
        changed['qualifications']['10']['qualification_receipt_sha256'] = gn.sha256(path.read_bytes())
        with self.assertRaisesRegex(gn.NeutralityError, 'unqualified command exit'):
            self.admit(changed)

    def test_native_lifecycle_changes_do_not_change_historical_digest(self):
        native = json.loads((self.evidence_root / 'native.json').read_text())
        native.update(status='recorded', completed_at='later', record_outcome='recorded')
        native['review_context'].update(episodes={'10': {}}, emitted_reviews=['new review'])
        self.put('native.json', native)
        self.assertEqual(self.admit()['identity']['base_sha'], self.base)
        # A mutable native path may have moved; snapshot still supplies proof.
        (self.evidence_root / 'native.json').unlink()
        self.admit()

    def test_overwrite_attempt_and_receipt_conflict_fail_without_replacing_pins(self):
        path = self.put('immutable.json', b'original')
        gn.create_only(path, b'original')
        with self.assertRaisesRegex(gn.NeutralityError, 'overwrite attempt'):
            gn.create_only(path, b'replacement')
        self.assertEqual(path.read_bytes(), b'original')
        gn.verify_task(self.admit())
        receipt_path = next(self.evidence_root.glob('task-10/*/receipt.json'))
        receipt_path.write_bytes(b'{}')
        before = self.pins()
        with self.assertRaisesRegex(gn.NeutralityError, 'sidecar mismatch'):
            gn.verify_task(self.admit())
        self.assertEqual(before, self.pins())

    def test_incomplete_request_and_changed_historical_pin_refuse(self):
        admitted = self.admit()
        (self.evidence_root / 'prelaunch.json').write_bytes(b'{}')
        with self.assertRaisesRegex(gn.NeutralityError, 'historical pin changed'):
            admitted['evidence'].unchanged()

    def test_regeneration_failure_and_out_of_domain_write(self):
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('#!/bin/sh\nprintf "broken" >&2\nexit 7\n')
        admitted = self.admit()
        with self.assertRaisesRegex(gn.NeutralityError, 'regeneration failed'):
            gn.verify_task(admitted)
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
        with self.assertRaisesRegex(gn.NeutralityError, 'incomplete existing receipt'):
            gn.verify_task(self.admit())
        shutil.rmtree(self.evidence_root / 'task-10')
        readme = self.repo / 'bindings/python/README.md'
        original = readme.read_bytes()
        readme.chmod(0o640)
        new_file = self.repo / 'unexpected.txt'
        cargo.write_text('#!/bin/sh\nprintf "clobbered" > "' + str(readme) + '"\n'
                         + f'chmod 644 "{readme}"\nprintf "new" > "{new_file}"\n')
        with self.assertRaisesRegex(gn.NeutralityError, 'outside generated domains') as failure:
            gn.verify_task(self.admit())
        self.assertIn('bindings/python/README.md', str(failure.exception))
        self.assertIn('unexpected.txt', str(failure.exception))
        self.assertEqual(readme.read_bytes(), original)
        self.assertEqual(readme.stat().st_mode & 0o777, 0o640)
        self.assertFalse(new_file.exists())

    def test_nine_exercise_incomplete_domains_refuse(self):
        result = self.cli('--exercise-nine')
        self.assertEqual(result.returncode, 2)
        self.assertIn('nine path outside declared generated domains', result.stderr)

    def test_task19_self_qualification_and_missing_accepted_binding_refuse(self):
        context = copy.deepcopy(self.context)
        for name in ('dispatches', 'prelaunch', 'dispatch_evidence', 'qualifications'):
            context[name]['19'] = context[name].pop('10')
        snapshot = json.loads((self.evidence_root / 'snapshot.json').read_text())
        snapshot['review_context']['tasks'][0]['task_id'] = 19
        path = self.put('snapshot.json', snapshot)
        context['dispatch_evidence']['19']['dispatch_snapshot_sha256'] = gn.sha256(path.read_bytes())
        envelope = json.loads((self.evidence_root / 'prelaunch.json').read_text())
        envelope.update(task_id='19', **context['dispatch_evidence']['19'])
        self.put('prelaunch.json', envelope)
        with self.assertRaisesRegex(gn.NeutralityError, 'binding_path'):
            self.admit(context, '19')
        receipt = json.loads((self.evidence_root / 'qualification.json').read_text())
        receipt['task_id'] = '19'
        path = self.put('qualification.json', receipt)
        context['qualifications']['19']['qualification_receipt_sha256'] = gn.sha256(path.read_bytes())
        with self.assertRaisesRegex(gn.NeutralityError, 'wrong task_id'):
            self.admit(context, '19')

    def test_stale_qualification_base_refused_before_generation(self):
        parent = self.base
        self.write('unrelated.txt', 'later base\n')
        later = self.commit()
        snapshot = json.loads((self.evidence_root / 'snapshot.json').read_text())
        snapshot['review_context']['base_sha'] = later
        path = self.put('snapshot.json', snapshot)
        context = copy.deepcopy(self.context)
        context['dispatch_evidence']['10']['dispatch_snapshot_sha256'] = gn.sha256(path.read_bytes())
        envelope = json.loads((self.evidence_root / 'prelaunch.json').read_text())
        envelope.update(base_sha=later, **context['dispatch_evidence']['10'])
        self.put('prelaunch.json', envelope)
        self.assertEqual(context['qualifications']['10']['source_identity']['base_sha'], parent)
        with self.assertRaisesRegex(gn.NeutralityError, 'stale qualification base'):
            self.admit(context)
        self.assertFalse((self.evidence_root / 'task-10').exists())

    def test_envelope_bound_to_other_snapshot_digest_refused(self):
        envelope = json.loads((self.evidence_root / 'prelaunch.json').read_text())
        envelope['dispatch_snapshot_sha256'] = '0' * 64
        self.put('prelaunch.json', envelope)
        with self.assertRaisesRegex(gn.NeutralityError, 'wrong dispatch_snapshot_sha256'):
            self.admit()

    def test_unset_context_variable_fails_closed(self):
        for value in ('', 'context.json'):
            result = subprocess.run([sys.executable, str(self.helper), 'verify', '--task', '10',
                                     '--context', value], cwd=self.repo, capture_output=True, text=True)
            with self.subTest(value=value):
                self.assertEqual(result.returncode, 2)
                self.assertIn('required absolute path', result.stderr)
        self.assertFalse((self.evidence_root / 'task-10').exists())


class HeadMoveRecoveryTests(unittest.TestCase):
    """Failed regeneration whose HEAD moves while it is failing.

    Recovery of a clean out-of-domain file reads its bytes from git, and the
    pre-fix helper read them from mutable HEAD. When HEAD advanced during the
    failing run, the first such path could not be recovered, the whole restore
    loop aborted behind it, and the recovery error replaced the domain
    violation that caused the run. The fixture commits these files before the
    base so they are clean, and the synthetic generator clobbers them.
    """

    head_move_fixture = True
    head_move_committed = ('handwritten/clean-a.txt', 'handwritten/clean-b.txt',
                           'handwritten/dirty.txt')
    head_move_clean = ('handwritten/clean-a.txt', 'handwritten/clean-b.txt')

    setUp = ContextTaskTests.setUp
    git = ScopeTests.git
    write = ScopeTests.write
    commit = ScopeTests.commit
    scope = ScopeTests.scope
    put = ContextTaskTests.put
    admit = ContextTaskTests.admit
    build_admission = ContextTaskTests.build_admission
    cli = ContextTaskTests.cli
    pins = ContextTaskTests.pins
    generation_script = ContextTaskTests.generation_script

    def test_failed_regeneration_across_head_move_restores_every_out_of_domain_path(self):
        repo = self.repo
        clean_a, clean_b = (repo / name for name in self.head_move_clean)
        clean = (clean_a, clean_b)
        for path in clean:
            path.chmod(0o644)
        before = {path: (path.read_bytes(), path.stat().st_mode & 0o777) for path in clean}
        self.assertEqual([mode for _, mode in before.values()], [0o644, 0o644])
        dirty = repo / 'handwritten/dirty.txt'
        dirty.write_text('operator edited bytes\n')
        dirty.chmod(0o640)
        untracked = self.write('handwritten/untracked.txt', 'operator untracked bytes\n')
        untracked.chmod(0o600)
        new_file = repo / 'handwritten/new-generated.txt'
        lines = ['#!/bin/sh', self.generation_script()]
        for path, mode in ((clean_a, 0o600), (clean_b, 0o755), (dirty, 0o600), (untracked, 0o600)):
            lines.append(f'printf "clobbered by generator" > "{path}"')
            lines.append(f'chmod {mode:o} "{path}"')
        lines.append(f'printf "new" > "{new_file}"')
        # Commit the CLOBBERED clean paths: an unchanged unrelated commit
        # would not make HEAD recovery stale. The pinned pre-run tree still
        # holds the original bytes.
        lines.append(f'git -C "{repo}" add -- {self.head_move_clean[0]} {self.head_move_clean[1]}')
        lines.append(f'git -C "{repo}" -c "user.name=Stage1 Test" '
                     '-c "user.email=stage1@example.invalid" commit -qm "commit clobbered paths"')
        lines.append('exit 9')
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('\n'.join(lines) + '\n')
        cargo.chmod(0o755)
        with self.assertRaises(gn.NeutralityError) as failure:
            gn.verify_task(self.admit())
        # The domain violation is the diagnosis; neither an unrecoverable path
        # nor its failure may replace it.
        self.assertIn('outside generated domains', str(failure.exception))
        self.assertIn('handwritten/clean-a.txt', str(failure.exception))
        for path in clean:
            with self.subTest(path=path.relative_to(repo).as_posix()):
                self.assertEqual(path.read_bytes(), before[path][0])
                self.assertEqual(path.stat().st_mode & 0o777, before[path][1])
        self.assertEqual(dirty.read_bytes(), b'operator edited bytes\n')
        self.assertEqual(dirty.stat().st_mode & 0o777, 0o640)
        self.assertEqual(untracked.read_bytes(), b'operator untracked bytes\n')
        self.assertEqual(untracked.stat().st_mode & 0o777, 0o600)
        self.assertFalse(new_file.exists())
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
        state_path = next(self.evidence_root.glob('task-10/*/regen-0.state.json'))
        self.assertEqual(json.loads(state_path.read_text())['out_of_domain'],
                         ['handwritten/clean-a.txt', 'handwritten/clean-b.txt',
                          'handwritten/dirty.txt', 'handwritten/new-generated.txt',
                          'handwritten/untracked.txt'])

    def test_unrecoverable_path_reports_beside_the_domain_violation(self):
        repo = self.repo
        clean_a, clean_b = (repo / name for name in self.head_move_clean)
        for path in (clean_a, clean_b):
            path.chmod(0o644)
        clean_b_before = clean_b.read_bytes()
        dirty = repo / 'handwritten/dirty.txt'
        dirty.write_text('operator edited bytes\n')
        dirty.chmod(0o640)
        new_file = repo / 'handwritten/new-generated.txt'
        lines = ['#!/bin/sh', self.generation_script(),
                 # A directory where the pre-run state had a clean file is not
                 # restorable: recovery must refuse the path, not overwrite it.
                 f'rm -f "{clean_a}"',
                 f'mkdir "{clean_a}"',
                 f'printf "clobbered by generator" > "{clean_b}"',
                 f'chmod 600 "{clean_b}"',
                 f'printf "clobbered by generator" > "{dirty}"',
                 f'chmod 600 "{dirty}"',
                 f'printf "new" > "{new_file}"',
                 'exit 9']
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('\n'.join(lines) + '\n')
        cargo.chmod(0o755)
        with self.assertRaises(gn.NeutralityError) as failure:
            gn.verify_task(self.admit())
        message = str(failure.exception)
        self.assertIn('outside generated domains', message)
        self.assertIn('recovery failed for: handwritten/clean-a.txt', message)
        self.assertIn('unsafe dirty-file recovery', message)
        # The path that could not be recovered is untouched, and every safe
        # restore behind it still happened.
        self.assertTrue(clean_a.is_dir())
        self.assertEqual(clean_b.read_bytes(), clean_b_before)
        self.assertEqual(clean_b.stat().st_mode & 0o777, 0o644)
        self.assertEqual(dirty.read_bytes(), b'operator edited bytes\n')
        self.assertEqual(dirty.stat().st_mode & 0o777, 0o640)
        self.assertFalse(new_file.exists())
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))


class RecoveryAbortTests(unittest.TestCase):
    """Real regeneration seams; independently safe recovery survives failures."""

    head_move_fixture = True
    head_move_committed = HeadMoveRecoveryTests.head_move_committed
    setUp = ContextTaskTests.setUp
    git = ScopeTests.git
    write = ScopeTests.write
    commit = ScopeTests.commit
    scope = ScopeTests.scope
    put = ContextTaskTests.put
    admit = ContextTaskTests.admit
    build_admission = ContextTaskTests.build_admission
    generation_script = ContextTaskTests.generation_script

    def prepare(self, extra=()):
        self.dirty = self.repo / 'handwritten/dirty.txt'
        self.dirty.write_bytes(b'operator edited bytes\n')
        self.dirty.chmod(0o640)
        self.untracked = self.write('handwritten/untracked.txt', 'operator untracked bytes\n')
        self.untracked.chmod(0o600)
        self.new = self.repo / 'handwritten/new-generated.txt'
        self.marker = self.evidence_root / 'generator-ran'
        script = ['#!/bin/sh', self.generation_script(), f'touch "{self.marker}"',
                  f'printf "clobbered" > "{self.dirty}"', f'chmod 600 "{self.dirty}"',
                  f'printf "clobbered" > "{self.untracked}"', f'chmod 644 "{self.untracked}"',
                  f'printf "new" > "{self.new}"', *extra, 'exit 9']
        (self.fake_bin / 'cargo').write_text('\n'.join(script) + '\n')
        return self.admit()

    def assert_safe_restores(self):
        self.assertEqual(self.dirty.read_bytes(), b'operator edited bytes\n')
        self.assertEqual(self.dirty.stat().st_mode & 0o777, 0o640)
        self.assertEqual(self.untracked.read_bytes(), b'operator untracked bytes\n')
        self.assertEqual(self.untracked.stat().st_mode & 0o777, 0o600)
        self.assertFalse(self.new.exists())
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))

    def test_tree_lookup_fault_allows_preflight_refusal_or_safe_recovery(self):
        admitted = self.prepare()
        original = gn._git_entries
        calls = []

        def lookup(repo, ref):
            calls.append(ref)
            # The old recovery-only seam reproduces B1. Also allow a repair
            # to preflight the tree (third lookup after request/dirt deltas).
            if self.marker.exists() or len(calls) == 3:
                raise gn.NeutralityError('injected recovery-tree lookup failure')
            return original(repo, ref)

        with patch.object(gn, '_git_entries', side_effect=lookup):
            with self.assertRaises(gn.NeutralityError) as failure:
                gn.verify_task(admitted)
        self.assertIn('injected recovery-tree lookup failure', str(failure.exception))
        if self.marker.exists():
            self.assertIn('outside generated domains', str(failure.exception))
        else:
            self.assertGreaterEqual(len(calls), 3)  # refused before mutation
        self.assert_safe_restores()

    def test_nonregular_post_snapshot_does_not_abort_safe_restores(self):
        unsafe = self.repo / 'handwritten/clean-a.txt'
        admitted = self.prepare([f'rm "{unsafe}"', f'mkfifo "{unsafe}"'])
        with self.assertRaises(gn.NeutralityError) as failure:
            gn.verify_task(admitted)
        self.assert_safe_restores()
        self.assertTrue(gn.stat.S_ISFIFO(unsafe.lstat().st_mode))
        message = str(failure.exception)
        self.assertIn('outside generated domains', message)
        self.assertIn('snapshot failed', message)
        self.assertIn('nonregular repository source: handwritten/clean-a.txt', message)
        self.assertIn('recovery failed for: handwritten/clean-a.txt', message)

    def test_unreadable_post_snapshot_leaves_unknown_file_untouched(self):
        unsafe = self.repo / 'handwritten/clean-a.txt'
        admitted = self.prepare([f'printf "unknown bytes" > "{unsafe}"'])
        original = Path.read_bytes

        def read(path):
            if self.marker.exists() and path == unsafe:
                raise PermissionError('injected unreadable file')
            return original(path)

        with patch.object(Path, 'read_bytes', read):
            with self.assertRaises(gn.NeutralityError) as failure:
                gn.verify_task(admitted)
        self.assert_safe_restores()
        self.assertEqual(unsafe.read_bytes(), b'unknown bytes')
        self.assertIn('snapshot failed', str(failure.exception))
        self.assertIn('injected unreadable file', str(failure.exception))
        self.assertIn('recovery failed for: handwritten/clean-a.txt', str(failure.exception))

    def test_evidence_and_domain_failures_are_visible_on_cli(self):
        admitted = self.prepare()
        original = gn.create_only

        def capture(path, data):
            if path.name == 'regen-0.stdout':
                # Actual immutable-evidence conflict, not an artificial cause.
                path.write_bytes(b'conflicting immutable stdout')
            original(path, data)

        from contextlib import redirect_stderr
        from io import StringIO
        diagnostics = StringIO()
        with patch.object(gn, 'admit_context', return_value=admitted), \
                patch.object(gn, 'create_only', side_effect=capture), redirect_stderr(diagnostics):
            result = gn.main(['verify', '--task', '10', '--context', str(self.context_path)])
        self.assertEqual(result, 2)
        self.assert_safe_restores()
        self.assertIn('outside generated domains', diagnostics.getvalue())
        self.assertIn('evidence capture failed', diagnostics.getvalue())
        self.assertIn('conflicting immutable evidence', diagnostics.getvalue())

    def test_clean_blob_failure_does_not_suppress_safe_recovery_or_evidence_error(self):
        clean = self.repo / 'handwritten/clean-a.txt'
        admitted = self.prepare([f'printf "clobbered clean" > "{clean}"'])
        original_git, original_capture = gn._git, gn.create_only

        def git(repo, *args):
            if self.marker.exists() and args[:2] == ('cat-file', 'blob'):
                raise gn.NeutralityError('injected clean blob failure')
            return original_git(repo, *args)

        def capture(path, data):
            if path.name == 'regen-0.stdout':
                path.write_bytes(b'conflicting immutable stdout')
            original_capture(path, data)

        with patch.object(gn, '_git', side_effect=git), \
                patch.object(gn, 'create_only', side_effect=capture):
            with self.assertRaises(gn.NeutralityError) as failure:
                gn.verify_task(admitted)
        self.assert_safe_restores()
        self.assertEqual(clean.read_bytes(), b'clobbered clean')
        message = str(failure.exception)
        self.assertIn('outside generated domains', message)
        self.assertIn('evidence capture failed', message)
        self.assertIn('conflicting immutable evidence', message)
        self.assertIn('recovery failed for: handwritten/clean-a.txt', message)
        self.assertIn('injected clean blob failure', message)

    def test_dirty_classification_and_recovery_use_one_immutable_base(self):
        clean = self.repo / 'handwritten/clean-a.txt'
        before = clean.read_bytes()
        admitted = self.prepare([f'printf "clobbered clean" > "{clean}"'])
        original = gn._code_delta
        calls = []

        def delta(repo, base, working):
            result = original(repo, base, working)
            calls.append(base)
            if len(calls) == 2:  # after regeneration's dirty classification
                clean.write_bytes(b'concurrent committed bytes')
                self.git('add', '--', 'handwritten/clean-a.txt')
                self.git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                         'commit', '-qm', 'concurrent clean-path change')
                clean.write_bytes(before)
            return result

        with patch.object(gn, '_code_delta', side_effect=delta):
            with self.assertRaises(gn.NeutralityError) as failure:
                gn.verify_task(admitted)
        self.assert_safe_restores()
        self.assertEqual(clean.read_bytes(), before)
        self.assertEqual(calls[1], self.base)
        self.assertNotIn('stale recovery object', str(failure.exception))

    def test_clean_git_recovery_preserves_actual_nondefault_permissions(self):
        clean = self.repo / 'handwritten/clean-a.txt'
        before = clean.read_bytes()
        clean.chmod(0o600)  # Git still sees a clean non-executable file.
        admitted = self.prepare([f'printf "clobbered clean" > "{clean}"', f'chmod 755 "{clean}"'])
        original = gn._git
        blobs = []

        def git(repo, *args):
            if self.marker.exists() and args[:2] == ('cat-file', 'blob'):
                blobs.append(args[2])
            return original(repo, *args)

        with patch.object(gn, '_git', side_effect=git):
            with self.assertRaisesRegex(gn.NeutralityError, 'outside generated domains'):
                gn.verify_task(admitted)
        self.assert_safe_restores()
        self.assertTrue(blobs, 'must exercise clean git-bytes recovery, not saved dirt')
        self.assertEqual(clean.read_bytes(), before)
        self.assertEqual(clean.stat().st_mode & 0o777, 0o600)


class PrivateRecoveryTests(unittest.TestCase):
    """Synthetic originals, real regeneration and genuine non-root denial."""

    head_move_fixture = True
    head_move_committed = HeadMoveRecoveryTests.head_move_committed
    setUp = ContextTaskTests.setUp
    git = ScopeTests.git
    write = ScopeTests.write
    commit = ScopeTests.commit
    scope = ScopeTests.scope
    put = ContextTaskTests.put
    admit = ContextTaskTests.admit
    build_admission = ContextTaskTests.build_admission
    generation_script = ContextTaskTests.generation_script
    prepare = RecoveryAbortTests.prepare
    assert_safe_restores = RecoveryAbortTests.assert_safe_restores

    def pending(self):
        common = Path(self.git('rev-parse', '--path-format=absolute', '--git-common-dir').strip())
        paths = list(common.glob('openxcvr-neutrality-recovery/*/pending.neutrality-recovery'))
        self.assertEqual(len(paths), 1)
        return paths[0], json.loads((paths[0] / 'manifest.json').read_bytes())

    def blocked_original(self):
        admitted = self.prepare([f'chmod 000 "{self.repo / "handwritten/dirty.txt"}"'])
        with self.assertRaisesRegex(gn.NeutralityError, 'undecidable post-generation path'):
            gn.verify_task(admitted)
        self.addCleanup(self.dirty.chmod, 0o640)
        return self.pending()

    def staging(self):
        parent = Path(tempfile.mkdtemp(prefix='neutrality-private-export-'))
        parent.chmod(0o700)
        self.addCleanup(shutil.rmtree, parent)
        return parent / 'export'

    def export_cli(self, m, destination, repo=None):
        return gn.main(['recovery-export', '--repo', str(repo or self.repo), '--origin', m['origin']['origin_id'],
                       '--snapshot', m['acquisition_id'], '--destination', str(destination)])

    def finalize_cli(self, m, repo=None):
        return gn.main(['recovery-finalize', '--repo', str(repo or self.repo), '--snapshot', m['acquisition_id']])

    def restore_fixture(self):
        self.dirty.chmod(0o640)
        self.dirty.write_bytes(b'operator edited bytes\n')

    def acquire_interrupted(self, stage):
        admitted = self.prepare()
        original = gn._PrivateDirectory.publish

        def publish(directory, name, data, **kw):
            if stage(name):
                raise OSError('injected acquisition interruption')
            return original(directory, name, data, **kw)

        with patch.object(gn._PrivateDirectory, 'publish', publish):
            with self.assertRaisesRegex(OSError, 'acquisition interruption'):
                gn.verify_task(admitted)
        self.assertFalse(self.marker.exists())
        return self.pending()

    def completed_directory(self):
        common = Path(self.git('rev-parse', '--path-format=absolute', '--git-common-dir').strip())
        paths = list(common.glob('openxcvr-neutrality-recovery/*/completed'))
        self.assertEqual(len(paths), 1)
        return paths[0]

    def interrupt_before_ready(self, admitted, tag):
        """A durable pending snapshot: complete payload inventory, no ready marker.

        Uses the real _regenerate seam and interrupts the ready publication
        itself, so nothing is generated and no source is touched.
        """
        original = gn._PrivateDirectory.publish

        def publish(directory, name, raw, **kw):
            if name == 'ready.json':
                raise OSError('injected hold before ready')
            return original(directory, name, raw, **kw)

        directory = self.evidence_root / tag
        directory.mkdir()
        with patch.object(gn._PrivateDirectory, 'publish', publish):
            with self.assertRaisesRegex(OSError, 'injected hold before ready'):
                gn._regenerate(admitted, directory, tag, [])
        return self.pending()

    def run_capturing_stderr(self, command, *args, patches=()):
        """Run a recovery entrypoint in-process and keep its stderr diagnostics."""
        from contextlib import ExitStack, redirect_stderr
        from io import StringIO
        captured = StringIO()
        with ExitStack() as stack:
            for target, attribute, replacement in patches:
                stack.enter_context(patch.object(target, attribute, replacement))
            stack.enter_context(redirect_stderr(captured))
            result = command(*args)
        return result, captured.getvalue()


    def test_real_dirty_chmod000_private_export(self):
        self.assertNotEqual(os.getuid(), 0, 'must execute as non-root')
        admitted = self.prepare([f'chmod 000 "{self.repo / "handwritten/dirty.txt"}"'])
        with self.assertRaises(gn.NeutralityError):
            gn.verify_task(admitted)
        with self.assertRaises(PermissionError):
            self.dirty.read_bytes()
        common = Path(self.git('rev-parse', '--path-format=absolute', '--git-common-dir').strip())
        pending = list(common.glob('openxcvr-neutrality-recovery/*/pending.neutrality-recovery'))
        self.assertEqual(len(pending), 1, 'initial originals must survive helper exit')
        manifest_raw = (pending[0] / 'manifest.json').read_bytes()
        manifest = json.loads(manifest_raw)
        destination_parent = Path(tempfile.mkdtemp(prefix='neutrality-private-export-'))
        destination_parent.chmod(0o700)
        self.addCleanup(shutil.rmtree, destination_parent)
        destination = destination_parent / 'export'
        self.assertEqual(gn.main(['recovery-export', '--repo', str(self.repo),
            '--origin', manifest['origin']['origin_id'], '--snapshot', manifest['acquisition_id'],
            '--destination', str(destination)]), 0)
        entry = next(e for e in manifest['entries'] if e['path'] == 'handwritten/dirty.txt')
        payload = (destination / entry['payload']).read_bytes()
        self.assertEqual(payload, b'operator edited bytes\n')
        self.assertEqual(gn.sha256(payload), entry['sha256'])
        self.assertEqual(self.dirty.stat().st_mode & 0o777, 0)
        self.assertTrue(pending[0].exists())
        self.assertEqual(self.untracked.read_bytes(), b'operator untracked bytes\n')
        self.assertFalse(self.new.exists())
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
        # Explicit fixture restoration, not recovery authority.
        self.dirty.chmod(0o640)


    def test_safe_siblings_and_unsafe_destination_untouched(self):
        pending, m = self.blocked_original()
        self.assertEqual(self.untracked.read_bytes(), b'operator untracked bytes\n')
        self.assertEqual(self.untracked.stat().st_mode & 0o777, 0o600)
        self.assertFalse(self.new.exists())
        self.assertEqual(self.finalize_cli(m), 2)
        self.assertEqual(self.dirty.stat().st_mode & 0o777, 0)
        self.assertTrue(pending.exists())
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))

    def test_export_does_not_restore_or_release(self):
        pending, m = self.blocked_original()
        dest = self.staging()
        before = {p.name: p.read_bytes() for p in pending.iterdir() if p.is_file()}
        self.assertEqual(self.export_cli(m, dest), 0)
        self.assertEqual(self.export_cli(m, dest), 2)  # create-only
        self.assertEqual(before, {p.name: p.read_bytes() for p in pending.iterdir() if p.is_file()})
        self.assertEqual(self.dirty.stat().st_mode & 0o777, 0)
        self.restore_fixture()
        self.assertEqual(self.finalize_cli(m), 0)
        self.assertFalse(pending.exists())
        self.assertEqual(self.finalize_cli(m), 0)  # completed readback
        self.assertEqual(self.export_cli(m, self.staging()), 0)

    def test_independent_format_decoder(self):
        pending, m = self.blocked_original()
        dest = self.staging()
        self.assertEqual(self.export_cli(m, dest), 0)
        # No helper decoder/hash import: ordinary JSON, hashlib, raw bytes.
        raw = (dest / 'manifest.json').read_bytes()
        manifest = json.loads(raw)
        self.assertEqual(raw, (json.dumps(manifest, sort_keys=True, separators=(',', ':'),
                                         ensure_ascii=True) + '\n').encode())
        ready = json.loads((dest / 'ready.json').read_bytes())
        self.assertEqual(ready['manifest_sha256'], hashlib.sha256(raw).hexdigest())
        self.assertEqual([e['ordinal'] for e in manifest['entries']], list(range(len(manifest['entries']))))
        for entry in manifest['entries']:
            if entry['kind'] == 'absent':
                self.assertIsNone(entry['payload'])
                continue
            payload = (dest / entry['payload']).read_bytes()
            self.assertEqual(len(payload), entry['size'])
            self.assertEqual(hashlib.sha256(payload).hexdigest(), entry['sha256'])
            self.assertEqual((dest / entry['payload']).stat().st_mode & 0o777, 0o600)
        self.assertEqual(dest.stat().st_mode & 0o777, 0o700)

    def test_export_after_evidence_and_linked_worktree_removal(self):
        # Real linked checkout origin, surviving main repo/common store. The
        # caller context is built for the linked checkout by the same harness,
        # so admission genuinely matches the caller instead of bypassing it.
        main = self.repo
        linked = main.parent / (main.name + '-linked')
        self.git('worktree', 'add', '--detach', str(linked))
        self.addCleanup(shutil.rmtree, linked, True)
        self.repo = linked
        self.build_admission(linked)
        admitted = self.prepare([f'chmod 000 "{linked / "handwritten/dirty.txt"}"'])
        with self.assertRaisesRegex(gn.NeutralityError, 'undecidable post-generation path'):
            gn.verify_task(admitted)
        pending, m = self.pending()
        # Independently retained synthetic originals, written by this fixture.
        expected = {'handwritten/dirty.txt': (b'operator edited bytes\n', 0o640),
                    'handwritten/untracked.txt': (b'operator untracked bytes\n', 0o600)}
        self.assertEqual([e['path'] for e in m['entries']], sorted(expected))
        for entry in m['entries']:
            self.assertEqual((entry['kind'], entry['mode']), ('file', expected[entry['path']][1]))
        # Explicit destruction of owned fixture, not a product operation: the
        # linked checkout, its administrative entry, and the old evidence tree.
        self.dirty.chmod(0o640)
        shutil.rmtree(linked)
        self.repo = main
        self.git('worktree', 'prune')
        shutil.rmtree(self.evidence_root)
        self.assertFalse(linked.exists())
        self.assertFalse(self.evidence_root.exists())
        self.assertNotIn(str(linked), self.git('worktree', 'list', '--porcelain'))
        destination = self.staging()
        with patch.object(gn, 'Evidence', side_effect=AssertionError('must not construct Evidence')):
            self.assertEqual(self.export_cli(m, destination), 0)
            self.assertEqual(self.finalize_cli(m), 2)
        self.assertTrue(pending.exists())
        # Exported originals are the retained synthetics, decoded independently.
        exported = json.loads((destination / 'manifest.json').read_bytes())
        self.assertEqual(exported['acquisition_id'], m['acquisition_id'])
        seen = set()
        for entry in exported['entries']:
            data, mode = expected[entry['path']]
            seen.add(entry['path'])
            self.assertEqual((entry['kind'], entry['mode']), ('file', mode))
            payload = (destination / entry['payload']).read_bytes()
            self.assertEqual(payload, data)
            self.assertEqual(len(payload), entry['size'])
            self.assertEqual(hashlib.sha256(payload).hexdigest(), entry['sha256'])
        self.assertEqual(seen, set(expected))

    def test_export_rejects_recorded_evidence_overlap_after_deletion(self):
        pending, m = self.blocked_original()
        root = self.evidence_root
        shutil.rmtree(root)
        original = gn._directory
        def directory(path, **kw):
            if gn._overlap(Path(path), root):
                raise AssertionError('old evidence or candidate inspected before lexical refusal')
            return original(path, **kw)
        for dest in (root, root / 'child', root.parent):
            with patch.object(gn, '_directory', directory), patch.object(gn, 'Evidence', side_effect=AssertionError):
                self.assertEqual(self.export_cli(m, dest), 2)
        # A character-prefix sibling is not an evidence descendant.
        sibling = Path(str(root) + '-copy')
        sibling.mkdir(mode=0o700)
        self.addCleanup(shutil.rmtree, sibling)
        with patch.object(gn, 'Evidence', side_effect=AssertionError):
            self.assertEqual(self.export_cli(m, sibling / 'export'), 0)
        raw = (pending / 'manifest.json').read_bytes()
        for value in (None, '/bad/../root', '//double', '/trailing/', '/bad\x00root'):
            changed = json.loads(raw)
            changed['provenance']['evidence_root'] = value
            (pending / 'manifest.json').write_bytes(gn._encoded(changed))
            self.assertEqual(self.export_cli(changed, self.staging()), 2)
        changed = json.loads(raw)
        del changed['provenance']['evidence_root']
        (pending / 'manifest.json').write_bytes(gn._encoded(changed))
        self.assertEqual(self.export_cli(changed, self.staging()), 2)

    def test_uid_owned_0775_metadata_without_chmod(self):
        common = self.repo / '.git'
        common.chmod(0o775)
        self.repo.chmod(0o775)
        with gn.PrivateRecovery(self.repo) as recovery:
            recovery.no_pending()
        self.assertEqual(common.stat().st_mode & 0o777, 0o775)
        self.assertEqual(self.repo.stat().st_mode & 0o777, 0o775)
        namespace = common / gn._RECOVERY
        self.assertEqual(namespace.stat().st_mode & 0o777, 0o700)
        namespace.chmod(0o755)
        with self.assertRaisesRegex(gn.NeutralityError, 'unsafe private directory'):
            gn.PrivateRecovery(self.repo)
        namespace.chmod(0o700)

    def test_root_and_sticky_ancestry(self):
        with gn._directory(Path('/')):
            pass
        self.assertEqual(Path('/').stat().st_uid, 0)
        self.assertTrue(Path('/tmp').stat().st_mode & stat.S_ISVTX)
        with gn.PrivateRecovery(self.repo):
            pass
        parent = self.staging().parent
        parent.chmod(0o777)
        with self.assertRaisesRegex(gn.NeutralityError, 'unsafe established ancestry'):
            with gn._directory(parent):
                pass
        parent.chmod(0o700)
        with patch.object(gn.sys, 'platform', 'darwin'):
            with self.assertRaisesRegex(gn.NeutralityError, 'Linux'):
                gn.PrivateRecovery(self.repo)

    def test_namespace_invisible_main_and_linked(self):
        before = gn._working_files(self.repo, [])
        with gn.PrivateRecovery(self.repo):
            pass
        self.assertEqual(before, gn._working_files(self.repo, []))
        linked = self.staging().parent / 'linked'
        self.git('worktree', 'add', '--detach', str(linked))
        baseline = gn._working_files(linked, [])
        with gn.PrivateRecovery(linked):
            pass
        self.assertEqual(baseline, gn._working_files(linked, []))

    def test_live_finalizer_and_export_excluded(self):
        pending, m = self.blocked_original()
        with gn.PrivateRecovery(self.repo):
            self.assertEqual(self.export_cli(m, self.staging()), 2)
            self.assertEqual(self.finalize_cli(m), 2)
        self.assertTrue(pending.exists())
        # Independent open descriptors in this process still contend.
        with gn.PrivateRecovery(self.repo):
            with self.assertRaisesRegex(gn.NeutralityError, 'busy private lease'):
                with gn.PrivateRecovery(self.repo):
                    pass

    def lifetime(self, kill_intermediate=False, inherit=True):
        # The actual _regenerate subprocess.run call is used. Only fixture cargo
        # hangs at a known child barrier; no unrelated process inspection.
        admitted = self.prepare()
        release = self.evidence_root / 'release'
        child_pid = self.evidence_root / 'child-pid'
        (self.fake_bin / 'cargo').write_text('#!/bin/sh\n' +
            f'exec python3 -c "import os,time;open(\'{child_pid}\',\'w\').write(str(os.getpid()));' +
            f'\nwhile not os.path.exists(\'{release}\'): time.sleep(.02)"\n')
        # fork isolates helper lifetime and preserves the fixture/admitted object.
        helper = os.fork()
        if helper == 0:
            os.setsid()
            if not inherit:
                run = gn.subprocess.run
                def without_fd(*args, **kw):
                    kw.pop('pass_fds', None)
                    return run(*args, **kw)
                gn.subprocess.run = without_fd
            try:
                gn.verify_task(admitted)
            except BaseException:
                pass
            os._exit(0)
        group = helper
        child = None
        try:
            until = time.monotonic() + 10
            while not child_pid.exists() and time.monotonic() < until:
                time.sleep(.02)
            self.assertTrue(child_pid.exists(), 'generator barrier not reached')
            child = int(child_pid.read_text())
            self.assertEqual(os.kill(child, 0), None)
            # Discover only our created mutator's ancestors via its status.
            ancestors = []
            current = child
            while current != helper:
                text = Path(f'/proc/{current}/status').read_text()
                current = int(next(l.split()[1] for l in text.splitlines() if l.startswith('PPid:')))
                self.assertGreater(current, 1)
                if current != helper:
                    ancestors.append(current)
            os.kill(helper, signal.SIGKILL)
            os.waitpid(helper, 0)
            helper = None
            if kill_intermediate:
                for pid in ancestors:
                    os.kill(pid, signal.SIGKILL)
            if inherit:
                with self.assertRaisesRegex(gn.NeutralityError, 'busy private lease'):
                    with gn.PrivateRecovery(self.repo):
                        pass
            else:
                with gn.PrivateRecovery(self.repo):
                    pass  # unsafe control exposes missing pass_fds
        finally:
            release.touch()
            if child:
                try:
                    os.kill(child, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            try:
                os.killpg(group, signal.SIGKILL)
            except ProcessLookupError:
                pass
            if helper:
                os.waitpid(helper, 0)

    def test_helper_killed_generator_alive(self):
        self.lifetime()

    def test_intermediate_killed_mutating_descendant_alive(self):
        self.lifetime(kill_intermediate=True)

    def test_missing_pass_fds_negative_control(self):
        self.lifetime(inherit=False)

    def test_partial_acquisition_and_pin_order(self):
        # contract-v3.1.md retains the name split by commit 46aec08. Each
        # component needs a fresh fixture: partial acquisition must not leave
        # receipt state that masks the independent pin-order check.
        problems = []
        for name in ('test_partial_acquisition_interrupted_payload_finalize',
                     'test_pin_order_refusal_before_generator_execution'):
            result = unittest.TestResult()
            PrivateRecoveryTests(name).run(result)
            problems.extend(f'{test.id()}\n{detail}'
                            for test, detail in result.errors + result.failures)
        if problems:
            self.fail('\n'.join(problems))

    def test_partial_acquisition_interrupted_payload_finalize(self):
        # Interrupted payload acquisition: export refuses the partial snapshot,
        # finalize completes it by republishing the missing payloads.
        pending, m = self.acquire_interrupted(lambda name: name.endswith('.bin'))
        self.assertEqual(self.export_cli(m, self.staging()), 2)
        self.assertEqual(self.finalize_cli(m), 0)  # republished from source originals
        self.assertFalse(pending.exists())
        with gn.PrivateRecovery(self.repo) as recovery:
            recovery.no_pending()

    def test_pin_order_refusal_before_generator_execution(self):
        # Isolated preconditions: the interrupted-acquisition fixture above
        # leaves a partial receipt directory behind, which would stop this case
        # at that guard instead of the pin check. This test arranges its own
        # valid admission and shares no receipt state with it.
        admitted = self.prepare()
        # Pin must be checked before pending creation, and again after ready.
        with patch.object(admitted['evidence'], 'unchanged', side_effect=gn.NeutralityError('pin drift')):
            with self.assertRaisesRegex(gn.NeutralityError, 'pin drift'):
                gn.verify_task(admitted)
        self.assertFalse(self.marker.exists(), 'generator ran before the pin check')
        self.assertEqual(self.dirty.read_bytes(), b'operator edited bytes\n')
        self.assertEqual(self.dirty.stat().st_mode & 0o777, 0o640)
        self.assertEqual(self.untracked.read_bytes(), b'operator untracked bytes\n')
        self.assertFalse(self.new.exists())
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
        with gn.PrivateRecovery(self.repo) as recovery:
            recovery.no_pending()

    def test_existing_truncated_payload_never_replaced(self):
        pending, m = self.acquire_interrupted(lambda name: name == 'ready.json')
        payload = next((pending / 'payloads').iterdir())
        payload.write_bytes(b'truncated')
        self.assertEqual(self.finalize_cli(m), 2)
        self.assertEqual(self.export_cli(m, self.staging()), 2)
        self.assertEqual(payload.read_bytes(), b'truncated')
        self.assertTrue(pending.exists())

    def test_acquisition_source_drift_barrier(self):
        # Source drift only: the readback and synchronization seams are covered
        # by the focused fault tests below.
        admitted = self.prepare()
        original = gn.PrivateRecovery.originals
        def drift(recovery, m):
            self.dirty.write_bytes(b'new operator bytes')
            return original(recovery, m)
        with patch.object(gn.PrivateRecovery, 'originals', drift):
            with self.assertRaisesRegex(gn.NeutralityError, 'unmatched'):
                gn.verify_task(admitted)
        self.assertFalse(self.marker.exists())
        pending, m = self.pending()
        self.assertEqual(self.finalize_cli(m), 2)
        self.assertTrue(pending.exists())

    def test_atomic_completion_interruptions_and_conflicts(self):
        pending, m = self.blocked_original()
        self.restore_fixture()
        with patch.object(gn, '_rename_private', side_effect=OSError('rename interrupted')):
            self.assertEqual(self.finalize_cli(m), 2)
        marker = (pending / 'completion.json').read_bytes()
        sequence = json.loads(marker)['sequence']
        self.dirty.write_bytes(b'newer source')
        self.assertEqual(self.finalize_cli(m), 2)  # marker cannot release mismatch
        self.assertEqual((pending / 'completion.json').read_bytes(), marker)
        self.restore_fixture()
        completed = pending.parent / 'completed'
        conflict = completed / (m['acquisition_id'] + gn._SUFFIX)
        conflict.mkdir(mode=0o700)
        self.assertEqual(self.finalize_cli(m), 2)
        self.assertTrue(pending.exists())
        conflict.rmdir()  # explicit owned-fixture obstruction removal
        self.assertEqual(self.finalize_cli(m), 0)
        self.assertEqual(json.loads((conflict / 'completion.json').read_bytes())['sequence'], sequence)
        self.assertFalse(pending.exists())

    def test_unmatched_and_malformed_never_pruned(self):
        pending, m = self.blocked_original()
        manifest = (pending / 'manifest.json').read_bytes()
        self.assertEqual(self.finalize_cli(m), 2)
        with self.assertRaisesRegex(gn.NeutralityError, 'pending'):
            gn.verify_task(self.admit())
        for raw in (b'{}\n', manifest.replace(b'"version":1', b'"version":2'),
                    manifest.replace(b'"version":1', b'"version":1,"version":1')):
            (pending / 'manifest.json').write_bytes(raw)
            self.assertEqual(self.finalize_cli(m), 2)
            self.assertEqual(self.export_cli(m, self.staging()), 2)
            self.assertEqual((pending / 'manifest.json').read_bytes(), raw)
        (pending / 'manifest.json').write_bytes(manifest)
        completed = pending.parent / 'completed'
        completed.mkdir(mode=0o700)
        unknown = completed / 'unknown'
        unknown.mkdir(mode=0o700)
        self.restore_fixture()
        self.assertEqual(self.finalize_cli(m), 0)
        self.assertTrue(unknown.exists())

    def test_completed_sequence_retention_and_config(self):
        self.git('config', 'openxcvr.neutralityRecoveryKeepCompleted', '1')
        admitted = self.prepare()
        for _ in range(2):
            with self.assertRaisesRegex(gn.NeutralityError, 'outside generated domains'):
                # Distinct evidence request not needed at direct _regenerate seam.
                directory = self.evidence_root / ('direct-' + str(_))
                directory.mkdir()
                gn._regenerate(admitted, directory, 'regen-0', [])
        common = self.repo / '.git'
        completed = next(common.glob('openxcvr-neutrality-recovery/*/completed'))
        self.assertEqual(len(list(completed.iterdir())), 1)
        sequence = json.loads(next(completed.glob('*/completion.json')).read_bytes())['sequence']
        self.assertEqual(sequence, 2)
        with gn.PrivateRecovery(self.repo) as recovery:
            self.assertEqual(recovery.sequence(), 2)
        for value in ('', '0', '-1', 'abc'):
            self.git('config', 'openxcvr.neutralityRecoveryKeepCompleted', value)
            with gn.PrivateRecovery(self.repo) as recovery:
                with self.assertRaisesRegex(gn.NeutralityError, 'invalid private retention'):
                    recovery.keep()
        self.git('config', 'openxcvr.neutralityRecoveryKeepCompleted', '1')
        self.git('config', '--add', 'openxcvr.neutralityRecoveryKeepCompleted', '2')
        with gn.PrivateRecovery(self.repo) as recovery:
            with self.assertRaises(gn.NeutralityError):
                recovery.keep()
        self.git('config', '--unset-all', 'openxcvr.neutralityRecoveryKeepCompleted')
        with gn.PrivateRecovery(self.repo) as recovery:
            self.assertEqual(recovery.keep(), 3)

    def test_export_finalize_and_receipt_immutability(self):
        # Existing full-mode suites remain the primary task/finish/nine coverage.
        pending, m = self.blocked_original()
        receipts = list(self.evidence_root.glob('task-10/*/regen-0.state.json'))
        original = receipts[0].read_bytes()
        self.assertEqual(self.export_cli(m, self.staging()), 0)
        self.restore_fixture()
        self.assertEqual(self.finalize_cli(m), 0)
        self.assertEqual(receipts[0].read_bytes(), original)
        with self.assertRaisesRegex(gn.NeutralityError, 'incomplete existing receipt'):
            gn.verify_task(self.admit())

    # C14: frozen contract-v3.1.md:585 names this test by its former method in
    # its focused qualification command; the alias keeps that command runnable.
    test_task_finish_nine_and_receipt_immutability = test_export_finalize_and_receipt_immutability

    def test_multiply_linked_source_refused_before_launch(self):
        # C10: recovery restores a clobbered path by unlinking and recreating it,
        # so a hard-link alias of that inode outside the worktree would keep the
        # failing generator's bytes while the acquisition still finalizes. A
        # multiply linked regular file refuses before any acquisition or launch.
        admitted = self.prepare()
        aliases = tempfile.TemporaryDirectory(prefix='neutrality-alias-')
        self.addCleanup(aliases.cleanup)
        alias = Path(aliases.name) / 'dirty-alias'
        os.link(self.dirty, alias)
        try:
            gn.verify_task(admitted)
        except gn.NeutralityError as error:
            message = str(error)
        else:
            self.fail('failing generator produced a receipt')
        common = Path(self.git('rev-parse', '--path-format=absolute', '--git-common-dir').strip())
        observed = dict(alias=alias.read_bytes(), generator_ran=self.marker.exists(),
                        private=sorted(p.name for p in common.glob('openxcvr-neutrality-recovery/*/*')
                                       if p.name != 'lease'),
                        refused='multiply linked' in message)
        self.assertEqual(observed, dict(alias=b'operator edited bytes\n', generator_ran=False,
                                        private=[], refused=True), message)
        self.assertEqual(self.dirty.read_bytes(), b'operator edited bytes\n')
        self.assertEqual(self.dirty.stat().st_mode & 0o777, 0o640)
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))

    def test_all_kinds_modes_and_absence_accounting(self):
        admitted = self.prepare()
        absent = self.repo / 'handwritten/clean-b.txt'
        absent.unlink()
        link = self.repo / 'handwritten/link'
        link.symlink_to('synthetic-target')
        empty = self.write('handwritten/empty', '')
        empty.chmod(0o604)
        # Ready publication interruption leaves source untouched.
        original = gn._PrivateDirectory.publish
        def publish(directory, name, raw, **kw):
            if name == 'ready.json':
                raise OSError('hold before ready')
            return original(directory, name, raw, **kw)
        with patch.object(gn._PrivateDirectory, 'publish', publish):
            with self.assertRaises(OSError):
                gn.verify_task(admitted)
        pending, m = self.pending()
        self.assertEqual({e['kind'] for e in m['entries']}, {'file', 'symlink', 'absent'})
        absent.write_bytes(b'newer data')
        self.assertEqual(self.finalize_cli(m), 2)
        self.assertEqual(absent.read_bytes(), b'newer data')  # no implicit deletion
        absent.unlink()
        empty.chmod(0o600)
        self.assertEqual(self.finalize_cli(m), 2)
        empty.chmod(0o604)
        link.unlink()
        link.symlink_to('changed-target')
        self.assertEqual(self.finalize_cli(m), 2)
        link.unlink()
        link.symlink_to('synthetic-target')
        self.assertEqual(self.finalize_cli(m), 0)

    def test_post_ready_pin_drift_prevents_generation(self):
        # The second pin check runs after a durable ready snapshot and before the
        # generator. The pin-order test above fails the pre-acquisition check;
        # this one lets acquisition publish valid ready and fails only the later
        # check, so the boundary reached is the post-ready one.
        admitted = self.prepare()
        common = Path(self.git('rev-parse', '--path-format=absolute', '--git-common-dir').strip())
        ready = 'openxcvr-neutrality-recovery/*/pending.neutrality-recovery/ready.json'
        original = gn.Evidence.unchanged
        pins = []

        def unchanged(evidence):
            pins.append(bool(list(common.glob(ready))))
            if pins[-1]:
                raise gn.NeutralityError('injected post-ready evidence pin drift')
            return original(evidence)

        with patch.object(gn.Evidence, 'unchanged', unchanged):
            with self.assertRaises(gn.NeutralityError) as failure:
                gn.verify_task(admitted)
        self.assertIn('injected post-ready evidence pin drift', str(failure.exception))
        # Check one saw no pending snapshot; check two saw a published ready marker.
        self.assertEqual(pins, [False, True])
        self.assertFalse(self.marker.exists(), 'generator ran after a failed post-ready pin check')
        self.assert_safe_restores()
        self.assertFalse(self.new.exists())
        pending, m = self.pending()
        self.assertTrue((pending / 'ready.json').is_file())
        with gn.PrivateRecovery(self.repo) as recovery:
            _, _, missing = recovery.snapshot(gn._PrivateDirectory(pending))
            self.assertEqual(missing, set(), 'ready declares a complete payload inventory')
        # Durable readiness means the originals are recoverable without source writes.
        destination = self.staging()
        self.assertEqual(self.export_cli(m, destination), 0)
        exported = json.loads((destination / 'manifest.json').read_bytes())
        self.assertEqual(exported['acquisition_id'], m['acquisition_id'])
        self.assertEqual((destination / 'payloads' / '00000000.bin').read_bytes(),
                         b'operator edited bytes\n')
        self.assertTrue(pending.exists())

    def test_actual_payload_readback_failure_prevents_generation(self):
        # Faults the real readback validation inside private publication, not
        # source drift: the payload file is written and then read back wrong.
        admitted = self.prepare()
        original = gn._PrivateDirectory.read
        faults = []

        def read(directory, name):
            if name.endswith('.bin'):
                faults.append(directory.path / name)
                return b'corrupted readback bytes'
            return original(directory, name)

        with patch.object(gn._PrivateDirectory, 'read', read):
            with self.assertRaises(gn.NeutralityError) as failure:
                gn.verify_task(admitted)
        self.assertEqual(len(faults), 1, faults)
        self.assertEqual(faults[0].parent.name, 'payloads')
        self.assertIn('private publication readback failed', str(failure.exception))
        self.assertIn(str(faults[0]), str(failure.exception))
        self.assertFalse(self.marker.exists(), 'generator ran after a failed payload readback')
        self.assert_safe_restores()
        pending, m = self.pending()
        self.assertEqual(faults[0], pending / 'payloads' / '00000000.bin')
        # Interrupted private data is preserved intact and not replaced.
        self.assertEqual((pending / 'payloads' / '00000000.bin').read_bytes(),
                         b'operator edited bytes\n')
        self.assertFalse((pending / 'payloads' / '00000001.bin').exists())
        self.assertFalse((pending / 'ready.json').exists())
        self.assertEqual([e['path'] for e in m['entries']],
                         ['handwritten/dirty.txt', 'handwritten/untracked.txt'])
        result, diagnostics = self.run_capturing_stderr(self.export_cli, m, self.staging())
        self.assertEqual(result, 2)
        self.assertIn('missing private payload', diagnostics)
        self.assertIn(str(pending / 'payloads'), diagnostics)
        # The intact payload still lets finalization publish ready and complete.
        self.assertEqual(self.finalize_cli(m), 0)
        self.assertFalse(pending.exists())

    def test_actual_payload_file_sync_failure_prevents_generation(self):
        # Faults the per-payload file fsync in publication, before the payload is
        # linked: the interrupted staging entry must survive untouched.
        admitted = self.prepare()
        original = os.fsync
        faults = []

        def fsync(fd):
            target = os.readlink(f'/proc/self/fd/{fd}')
            if stat.S_ISREG(os.fstat(fd).st_mode) and Path(target).parent.name == 'payloads':
                faults.append(target)
                raise OSError(errno.EIO, 'injected payload file sync failure')
            return original(fd)

        with patch.object(gn.os, 'fsync', fsync):
            with self.assertRaises(OSError) as failure:
                gn.verify_task(admitted)
        self.assertEqual(len(faults), 1, faults)
        self.assertEqual(Path(faults[0]).parent.name, 'payloads')
        self.assertTrue(Path(faults[0]).name.startswith('.staging-'))
        self.assertIn('injected payload file sync failure', str(failure.exception))
        self.assertFalse(self.marker.exists(), 'generator ran after a failed payload fsync')
        self.assert_safe_restores()
        pending, m = self.pending()
        preserved = [p.name for p in (pending / 'payloads').iterdir()]
        self.assertEqual(len(preserved), 1)
        self.assertTrue(preserved[0].startswith('.staging-'),
                        'interrupted staging must be preserved, not cleaned')
        self.assertFalse((pending / 'ready.json').exists())
        # An unexplained staging entry is refused, never replayed or deleted.
        result, diagnostics = self.run_capturing_stderr(self.finalize_cli, m)
        self.assertEqual(result, 2)
        self.assertIn('unknown private payload', diagnostics)
        self.assertEqual([p.name for p in (pending / 'payloads').iterdir()], preserved)
        self.assertTrue(pending.exists())

    def test_actual_payload_directory_sync_failure_through_cli(self):
        # Faults the payloads-directory fsync that followed a real link of the
        # payload, and exercises the real command boundary for exit behaviour.
        admitted = self.prepare()
        original = os.fsync
        faults = []

        def fsync(fd):
            target = os.readlink(f'/proc/self/fd/{fd}')
            if (stat.S_ISDIR(os.fstat(fd).st_mode) and Path(target).name == 'payloads'
                    and os.listdir(fd)):
                faults.append(target)
                raise OSError(errno.EIO, 'injected payload directory sync failure')
            return original(fd)

        result, diagnostics = self.run_capturing_stderr(
            gn.main, ['verify', '--task', '10', '--context', str(self.context_path)],
            patches=[(gn, 'admit_context', lambda *a, **kw: admitted),
                     (gn.os, 'fsync', fsync)])
        self.assertEqual(result, 2)
        self.assertEqual(len(faults), 1, faults)
        self.assertEqual(Path(faults[0]).name, 'payloads')
        self.assertIn('[Errno 5] injected payload directory sync failure', diagnostics)
        self.assertFalse(self.marker.exists(), 'generator ran after a failed directory fsync')
        self.assert_safe_restores()
        pending, m = self.pending()
        self.assertEqual([p.name for p in (pending / 'payloads').iterdir()], ['00000000.bin'])
        self.assertEqual((pending / 'payloads' / '00000000.bin').read_bytes(),
                         b'operator edited bytes\n')
        self.assertFalse((pending / 'ready.json').exists())
        with gn.PrivateRecovery(self.repo) as recovery:
            _, _, missing = recovery.snapshot(gn._PrivateDirectory(pending), partial=True)
            # The write stopped after the first payload was linked and synced.
            self.assertEqual(missing, {'00000001.bin'})
        self.assertEqual(self.export_cli(m, self.staging()), 2)
        self.assertEqual(self.finalize_cli(m), 0)
        self.assertFalse(pending.exists())

    def test_git_tracked_deletion_absence_capture_export_and_refusal(self):
        # A genuinely Git-tracked path outside every generated domain, deleted
        # before capture: the initial saved entry is None and the manifest
        # records an absence marker rather than a payload.
        tracked = 'bindings/python/README.md'
        self.assertEqual(self.git('ls-files', '--error-unmatch', tracked), tracked)
        (self.repo / tracked).unlink()
        self.assertFalse((self.repo / tracked).exists())
        # Unstaged tracked deletion, seen by Git as exactly that.
        self.assertEqual(self.git('diff', '--name-only', '--diff-filter=D'), tracked)
        admitted = self.prepare()
        # A generator that leaves the working tree alone, so the captured
        # deletion is the only out-of-domain change this run reports.
        (self.fake_bin / 'cargo').write_text('#!/bin/sh\nprintf "fake generation\\n"\n')
        directory = self.evidence_root / 'tracked-deletion'
        directory.mkdir()
        command, state = gn._regenerate(admitted, directory, 'regen-0', [])
        self.assertEqual(command['exit'], 0)
        self.assertEqual(state['out_of_domain'], [])
        self.assertFalse((self.repo / tracked).exists(), 'a captured deletion must stay deleted')
        completed = self.completed_directory()
        snapshots = list(completed.glob('*' + gn._SUFFIX))
        self.assertEqual(len(snapshots), 1)
        manifest = json.loads((snapshots[0] / 'manifest.json').read_bytes())
        matching = [e for e in manifest['entries'] if e['path'] == tracked]
        self.assertEqual(len(matching), 1, f'no captured absence entry for {tracked}')
        entry = matching[0]
        self.assertEqual(entry['kind'], 'absent')
        self.assertEqual([entry[k] for k in ('mode', 'payload', 'size', 'sha256')],
                         [None, None, None, None])
        payloads = {Path(e['payload']).name for e in manifest['entries'] if e['payload'] is not None}
        self.assertEqual({p.name for p in (snapshots[0] / 'payloads').iterdir()}, payloads)
        self.assertEqual(len(manifest['entries']), len(payloads) + 1)
        # Export preserves the absence marker, and decodes without helper imports.
        destination = self.staging()
        self.assertEqual(self.export_cli(manifest, destination), 0)
        raw = (destination / 'manifest.json').read_bytes()
        exported = json.loads(raw)
        self.assertEqual(raw, (json.dumps(exported, sort_keys=True, separators=(',', ':'),
                                         ensure_ascii=True) + '\n').encode())
        exported_entry = [e for e in exported['entries'] if e['path'] == tracked]
        self.assertEqual(len(exported_entry), 1)
        exported_entry = exported_entry[0]
        self.assertEqual(exported_entry['kind'], 'absent')
        self.assertEqual([exported_entry[k] for k in ('mode', 'payload', 'size', 'sha256')],
                         [None, None, None, None])
        self.assertEqual({p.name for p in (destination / 'payloads').iterdir()}, payloads)
        for exported_file in exported['entries']:
            if exported_file['payload'] is None:
                continue
            data = (destination / exported_file['payload']).read_bytes()
            self.assertEqual(len(data), exported_file['size'])
            self.assertEqual(hashlib.sha256(data).hexdigest(), exported_file['sha256'])
        self.assertFalse((self.repo / tracked).exists(), 'export must not restore source')
        # A second snapshot, interrupted before ready, captures the same absence
        # while it is still pending.
        pending, second = self.interrupt_before_ready(admitted, 'second-deletion')
        self.assertFalse((self.repo / tracked).exists())
        newer = b'newer operator replacement\n'
        (self.repo / tracked).write_bytes(newer)
        result, diagnostics = self.run_capturing_stderr(self.finalize_cli, second)
        self.assertEqual(result, 2)
        self.assertIn('original absence unmatched: ' + tracked, diagnostics)
        self.assertEqual((self.repo / tracked).read_bytes(), newer,
                         'an absence marker never authorizes deleting newer data')
        self.assertTrue(pending.exists())
        # With the newer data removed, the preserved snapshot still completes.
        (self.repo / tracked).unlink()
        self.assertEqual(self.finalize_cli(second), 0)
        self.assertFalse(pending.exists())

    def test_completed_readback_failure_is_fatal_without_receipt(self):
        admitted = self.prepare()
        (self.fake_bin / 'cargo').write_text('#!/bin/sh\n' + self.generation_script() + '\n')
        original = gn._PrivateDirectory.read
        faults = []

        def read(directory, name):
            if directory.path.parent.name == 'completed' and name == 'manifest.json':
                faults.append(directory.path)
                raise OSError(errno.EIO, 'injected completed snapshot readback failure')
            return original(directory, name)

        with patch.object(gn.PrivateRecovery, 'prune') as prune:
            result, diagnostics = self.run_capturing_stderr(
                gn.main, ['verify', '--task', '10', '--context', str(self.context_path)],
                patches=[(gn, 'admit_context', lambda *a, **kw: admitted),
                         (gn._PrivateDirectory, 'read', read)])
            prune.assert_not_called()
        self.assertEqual(len(faults), 1, faults)
        self.assertEqual(result, 2)
        self.assertIn('injected completed snapshot readback failure', diagnostics)
        self.assertNotIn('WARNING', diagnostics)
        self.assert_safe_restores()
        completed = self.completed_directory()
        self.assertEqual(list(completed.iterdir()), faults)
        with gn.PrivateRecovery(self.repo) as recovery:
            recovery.no_pending()
            recovery.snapshot(gn._PrivateDirectory(faults[0]))
            self.assertEqual(recovery.sequence(), 1)

    def test_completion_parent_sync_failures_are_fatal_without_receipt(self):
        # Fault each real parent-directory fsync after renameat2 has moved the
        # snapshot, not an acquisition/publication sync before completion.
        for parent in ('origin', 'completed'):
            with self.subTest(parent=parent):
                admitted = self.prepare()
                (self.fake_bin / 'cargo').write_text('#!/bin/sh\n' + self.generation_script() + '\n')
                original = os.fsync
                faults = []

                def fsync(fd):
                    path = Path(os.readlink(f'/proc/self/fd/{fd}'))
                    completed = path if parent == 'completed' else path / 'completed'
                    selected = (path.name == 'completed' if parent == 'completed'
                                else path.parent.name == gn._RECOVERY)
                    if (stat.S_ISDIR(os.fstat(fd).st_mode) and selected
                            and completed.is_dir() and list(completed.glob('*' + gn._SUFFIX))):
                        faults.append(path)
                        raise OSError(errno.EIO, 'injected completion parent sync failure')
                    return original(fd)

                with patch.object(gn.PrivateRecovery, 'prune') as prune:
                    result, diagnostics = self.run_capturing_stderr(
                        gn.main, ['verify', '--task', '10', '--context', str(self.context_path)],
                        patches=[(gn, 'admit_context', lambda *a, **kw: admitted),
                                 (gn.os, 'fsync', fsync)])
                    prune.assert_not_called()
                self.assertEqual(len(faults), 1, faults)
                self.assertEqual(result, 2)
                self.assertIn('uncertain completion at ', diagnostics)
                self.assertIn('injected completion parent sync failure', diagnostics)
                self.assertNotIn('WARNING', diagnostics)
                self.assert_safe_restores()
                completed = self.completed_directory()
                snapshots = list(completed.glob('*' + gn._SUFFIX))
                self.assertEqual(len(snapshots), 1)
                self.assertIn(str(snapshots[0]), diagnostics)
                with gn.PrivateRecovery(self.repo) as recovery:
                    recovery.no_pending()
                    recovery.snapshot(gn._PrivateDirectory(snapshots[0]))
                    self.assertEqual(recovery.sequence(), 1)
                # Remove only this subcase's synthetic runtime/evidence so the
                # next fault reaches a fresh completion, not receipt replay.
                shutil.rmtree(completed.parent)
                shutil.rmtree(self.evidence_root / 'task-10')

    def test_unexpected_prune_inventory_error_propagates(self):
        from contextlib import redirect_stderr, redirect_stdout
        from io import StringIO
        admitted = self.prepare()
        pending, m = self.interrupt_before_ready(admitted, 'programmer-error')
        original_listdir, original_rename = os.listdir, gn._rename_private
        state = {'rename_done': False}
        faults = []
        diagnostics, output = StringIO(), StringIO()

        def rename(source, name, target, new):
            result = original_rename(source, name, target, new)
            state['rename_done'] = True
            return result

        def listdir(path):
            if isinstance(path, int):
                target = Path(os.readlink(f'/proc/self/fd/{path}'))
                if state['rename_done'] and target.name == 'completed':
                    faults.append(target)
                    raise RuntimeError('injected pruning programmer error')
            return original_listdir(path)

        with (patch.object(gn, '_rename_private', rename),
              patch.object(gn.os, 'listdir', listdir),
              redirect_stderr(diagnostics), redirect_stdout(output)):
            with self.assertRaisesRegex(RuntimeError, 'injected pruning programmer error'):
                self.finalize_cli(m)
        self.assertEqual(len(faults), 1, faults)
        self.assertEqual(diagnostics.getvalue(), '')
        self.assertEqual(output.getvalue(), '', 'programmer error must not report success')
        self.assertFalse(pending.exists())
        newest = faults[0] / (m['acquisition_id'] + gn._SUFFIX)
        with gn.PrivateRecovery(self.repo) as recovery:
            recovery.snapshot(gn._PrivateDirectory(newest))
            self.assertEqual(recovery.sequence(), 1)
        self.assert_safe_restores()

    def test_prune_removal_failure_warns_and_retains_completion(self):
        # Retention requiring a real pruning removal, faulted after the atomic
        # completion: the newly completed snapshot stays valid and retained and
        # the failed removal only warns.
        self.git('config', 'openxcvr.neutralityRecoveryKeepCompleted', '1')
        admitted = self.prepare()
        first_pending, first = self.interrupt_before_ready(admitted, 'first-snapshot')
        self.assertEqual(self.finalize_cli(first), 0)
        completed = self.completed_directory()
        second_pending, second = self.interrupt_before_ready(admitted, 'second-snapshot')
        original = os.unlink
        faults = []

        def unlink(path, *, dir_fd=None):
            if dir_fd is not None:
                target = os.readlink(f'/proc/self/fd/{dir_fd}')
                if '/completed/' in target:
                    faults.append(target + '/' + os.fsdecode(path))
                    raise OSError(errno.EIO, 'injected prune removal failure')
                return original(path, dir_fd=dir_fd)
            return original(path)

        result, diagnostics = self.run_capturing_stderr(
            self.finalize_cli, second, patches=[(gn.os, 'unlink', unlink)])
        self.assertEqual(len(faults), 1, faults)
        self.assertFalse(second_pending.exists())
        newest = completed / (second['acquisition_id'] + gn._SUFFIX)
        oldest = completed / (first['acquisition_id'] + gn._SUFFIX)
        self.assertEqual(json.loads((newest / 'completion.json').read_bytes())['sequence'], 2)
        self.assertTrue(oldest.is_dir(), 'unpruned excess state is preserved, not deleted')
        self.assertEqual({p.name for p in completed.iterdir()}, {newest.name, oldest.name})
        with gn.PrivateRecovery(self.repo) as recovery:
            self.assertEqual(recovery.sequence(), 2)
        self.assertEqual(result, 0, 'a prune removal failure must warn, not fail the run')
        self.assertIn('prune failed at ' + str(oldest), diagnostics)

    def test_prune_inventory_failure_warns_rather_than_invalidating_completion(self):
        # Historical red record (first written 2026-10-09, for frozen H commit
        # 153dadc9): this test began as a red checkpoint reproducing the defect
        # below and now passes against the repaired helper, so it is not a
        # current NOT ACCEPTED verdict. The line citations that follow describe
        # 153dadc9, not the current source. Contract section 6: "Prune
        # failure warns and does not invalidate otherwise successful completion."
        # Against frozen H (153dadc9), with the fault armed only after
        # the atomic completion rename and fsync, finalization exited 2 and
        # printed "ERROR: [Errno 5] injected completed inventory read failure",
        # while the completion marker, the newest retained snapshot and the
        # excess unpruned snapshot were all intact. H.prune called
        # sorted(completed.names()) at H:1472, outside the per-entry handlers at
        # H:1479-1490 and H:1509-1513, and H.finalize reached prune at H:1464
        # after the rename and its validation.
        self.git('config', 'openxcvr.neutralityRecoveryKeepCompleted', '1')
        admitted = self.prepare()
        first_pending, first = self.interrupt_before_ready(admitted, 'first-snapshot')
        self.assertEqual(self.finalize_cli(first), 0)
        completed = self.completed_directory()
        second_pending, second = self.interrupt_before_ready(admitted, 'second-snapshot')
        original_listdir, original_rename = os.listdir, gn._rename_private
        state = {'rename_done': False}
        faults = []

        def rename(source, name, target, new):
            result = original_rename(source, name, target, new)
            if target.path.name == 'completed':
                state['rename_done'] = True  # armed only after rename and fsync
            return result

        def listdir(path):
            if isinstance(path, int):
                target = os.readlink(f'/proc/self/fd/{path}')
                if state['rename_done'] and Path(target).name == 'completed':
                    faults.append(target)
                    raise OSError(errno.EIO, 'injected completed inventory read failure')
            return original_listdir(path)

        result, diagnostics = self.run_capturing_stderr(
            self.finalize_cli, second,
            patches=[(gn, '_rename_private', rename), (gn.os, 'listdir', listdir)])
        # Evidence first: the completion this run performed already succeeded.
        self.assertEqual(len(faults), 1, faults)
        self.assertFalse(second_pending.exists())
        newest = completed / (second['acquisition_id'] + gn._SUFFIX)
        oldest = completed / (first['acquisition_id'] + gn._SUFFIX)
        self.assertTrue((newest / 'manifest.json').is_file())
        self.assertEqual(json.loads((newest / 'completion.json').read_bytes())['sequence'], 2)
        self.assertTrue(oldest.is_dir(), 'unpruned excess state is preserved, not deleted')
        with gn.PrivateRecovery(self.repo) as recovery:
            self.assertEqual(recovery.sequence(), 2)
        self.assertEqual(result, 0,
                         'a pruning failure must warn rather than invalidate an otherwise '
                         'successful completion (contract section 6); actual exit '
                         f'{result} with stderr: {diagnostics!r}')
        self.assertIn('WARNING', diagnostics)


class EvidenceNamespaceOverlapTests(unittest.TestCase):
    """C8: the admitted evidence root never overlaps the private namespace.

    Contract section 1 refuses an evidence root that encompasses the private
    namespace, independent of whether this run has any originals to save, so
    a clean worktree is refused before generation, logs, receipts, private
    state or nine removal.
    """
    nine_fixture = True
    setUp = ContextTaskTests.setUp
    git = ScopeTests.git
    write = ScopeTests.write
    commit = ScopeTests.commit
    scope = ScopeTests.scope
    put = ContextTaskTests.put
    admit = ContextTaskTests.admit
    build_admission = ContextTaskTests.build_admission
    configure_nine = ContextTaskTests.configure_nine
    generation_script = ContextTaskTests.generation_script

    def retarget(self, repo, evidence_root):
        """Admit `repo` against `evidence_root`; the fake generator follows it."""
        self.repo = repo
        self.output = repo / ROOT / 'tracked.py'
        self.marker = self.fake_bin / 'generator-ran'
        (self.fake_bin / 'cargo').write_text('#!/bin/sh\n' + self.generation_script()
                                             + f'touch "{self.marker}"\n')
        self.evidence_root = evidence_root
        self.build_admission(repo)
        self.common = Path(self.git('rev-parse', '--path-format=absolute', '--git-common-dir')).resolve()
        self.namespace = self.common / gn._RECOVERY
        self.assertTrue(gn._overlap(self.namespace, evidence_root))

    def clean_clone_inside_evidence(self):
        clone = self.evidence_root / 'nested-clone'
        subprocess.run(['git', 'clone', '-q', str(self.repo), str(clone)], check=True)
        self.retarget(clone, self.evidence_root)

    def clean_linked_with_common_evidence(self):
        linked = self.evidence_root / 'linked'
        self.git('worktree', 'add', '--detach', str(linked), self.base)
        common = Path(self.git('rev-parse', '--path-format=absolute', '--git-common-dir')).resolve()
        self.retarget(linked, common)

    def linked_with_evidence_in_namespace(self, name):
        # Equal (name None) and descendant forms: a linked worktree keeps the
        # common directory, and so the namespace, outside its own worktree.
        linked = self.evidence_root / 'linked'
        self.git('worktree', 'add', '--detach', str(linked), self.base)
        with gn.PrivateRecovery(linked):
            pass
        common = Path(self.git('rev-parse', '--path-format=absolute', '--git-common-dir')).resolve()
        root = common / gn._RECOVERY
        if name is not None:
            root = root / name
            root.mkdir()
        self.retarget(linked, root)

    def private_state(self):
        return sorted(str(p) for p in self.namespace.rglob('*')) if self.namespace.exists() else None

    def assert_refused_before_anything(self, exercise_nine):
        admitted = self.admit()
        private = self.private_state()
        self.assertEqual(gn._code_delta(self.repo, self.base, gn._working_files(self.repo, [])), [])
        nine = {p: (self.repo / p).read_bytes() for p in gn.NINE_PATHS}
        with patch.object(gn.PrivateRecovery, 'acquire', autospec=True,
                          side_effect=gn.PrivateRecovery.acquire) as acquire, \
             patch.object(Path, 'unlink', autospec=True, side_effect=Path.unlink) as unlink, \
             patch.object(gn, '_regenerate', wraps=gn._regenerate) as regenerate:
            with self.assertRaisesRegex(gn.NeutralityError, 'evidence overlaps private namespace'):
                gn.verify_task(admitted, exercise_nine=exercise_nine)
            acquire.assert_not_called()
            unlink.assert_not_called()
            regenerate.assert_not_called()
        self.assertFalse(self.marker.exists(), 'generator ran')
        self.assertFalse((self.evidence_root / 'task-10').exists(), 'evidence written')
        self.assertFalse(list(self.evidence_root.rglob('receipt.json')))
        self.assertEqual(self.private_state(), private, 'private state created before refusal')
        self.assertEqual({p: (self.repo / p).read_bytes() for p in gn.NINE_PATHS}, nine)

    def test_clean_worktree_under_evidence_root_refuses(self):
        self.clean_clone_inside_evidence()
        self.assert_refused_before_anything(exercise_nine=False)

    def test_clean_worktree_under_evidence_root_refuses_nine(self):
        self.clean_clone_inside_evidence()
        self.assert_refused_before_anything(exercise_nine=True)

    def test_linked_worktree_common_dir_evidence_refuses(self):
        self.clean_linked_with_common_evidence()
        self.assert_refused_before_anything(exercise_nine=False)

    def test_linked_worktree_common_dir_evidence_refuses_nine(self):
        self.clean_linked_with_common_evidence()
        self.assert_refused_before_anything(exercise_nine=True)

    def test_evidence_root_equal_to_namespace_refuses(self):
        self.linked_with_evidence_in_namespace(None)
        self.assert_refused_before_anything(exercise_nine=False)

    def test_evidence_root_inside_namespace_refuses(self):
        self.linked_with_evidence_in_namespace('evidence')
        self.assert_refused_before_anything(exercise_nine=True)

    def test_direct_nine_entry_refuses(self):
        # The leased nine entry, called without an outer verification, refuses too.
        self.clean_clone_inside_evidence()
        admitted = self.admit()
        directory = Path(tempfile.mkdtemp(prefix='neutrality-overlap-nine-'))
        self.addCleanup(shutil.rmtree, directory)
        nine = {p: (self.repo / p).read_bytes() for p in gn.NINE_PATHS}
        with patch.object(Path, 'unlink', autospec=True, side_effect=Path.unlink) as unlink:
            with self.assertRaisesRegex(gn.NeutralityError, 'evidence overlaps private namespace'):
                gn._exercise_nine(admitted, directory, [], list(gn.NINE_PATHS), None)
            unlink.assert_not_called()
        self.assertEqual({p: (self.repo / p).read_bytes() for p in gn.NINE_PATHS}, nine)
        self.assertFalse(self.namespace.exists())

    def test_acquire_refuses_with_and_without_originals(self):
        # Unit level: the acquisition itself refuses an overlapping root
        # whether or not there is an original to save.
        root = self.repo.parent
        admitted = {'evidence': gn.Evidence(self.repo, root)}
        before = {'bindings/python/README.md': dict(kind='file', mode=0o644,
                                                     sha256=gn.sha256(b'dirty\n'))}
        saved = {'bindings/python/README.md': ('file', b'dirty\n', 0o644)}
        with gn.PrivateRecovery(self.repo) as recovery:
            self.assertTrue(gn._overlap(recovery.namespace.path, root))
            for originals, prior in (({}, {}), (saved, before)):
                with self.subTest(saved=bool(originals)):
                    with self.assertRaisesRegex(gn.NeutralityError, 'evidence overlaps private namespace'):
                        recovery.acquire(originals, prior, admitted, 'regen-0', self.base)
            recovery.no_pending()

    def test_disjoint_evidence_root_still_verifies(self):
        receipt = gn.verify_task(self.admit())
        self.assertEqual([c['exit'] for c in receipt['commands']], [0, 0])
        receipt = gn.verify_task(self.admit(), exercise_nine=True)
        self.assertEqual(receipt['nine_exercise']['paths'], list(gn.NINE_PATHS))


class NineTests(unittest.TestCase):
    nine_fixture = True
    setUp = ContextTaskTests.setUp
    git = ScopeTests.git
    write = ScopeTests.write
    commit = ScopeTests.commit
    scope = ScopeTests.scope
    put = ContextTaskTests.put
    admit = ContextTaskTests.admit
    build_admission = ContextTaskTests.build_admission
    cli = ContextTaskTests.cli
    pins = ContextTaskTests.pins
    configure_nine = ContextTaskTests.configure_nine
    generation_script = ContextTaskTests.generation_script

    def test_caller_rustc_wrapper_refuses_before_any_mutation(self):
        admitted = self.admit()
        before = gn._working_files(self.repo, [])
        pins = self.pins()
        common = Path(self.git('rev-parse', '--path-format=absolute', '--git-common-dir'))
        private = common / 'openxcvr-neutrality-recovery'
        self.assertFalse(private.exists())
        with patch.dict(os.environ, {'RUSTC_WRAPPER': str(self.evidence_root / 'wrapper')}), \
             patch.object(gn.PrivateRecovery, 'acquire', autospec=True,
                          side_effect=gn.PrivateRecovery.acquire) as acquire, \
             patch.object(Path, 'unlink', autospec=True, side_effect=Path.unlink) as unlink, \
             patch.object(gn, '_regenerate', wraps=gn._regenerate) as regenerate:
            with self.assertRaisesRegex(gn.NeutralityError, 'RUSTC_WRAPPER'):
                gn.verify_task(admitted, exercise_nine=True)
            acquire.assert_not_called()
            unlink.assert_not_called()
            regenerate.assert_not_called()
        self.assertEqual(gn._working_files(self.repo, []), before)
        self.assertEqual(self.pins(), pins)
        self.assertFalse(private.exists(), 'private state was created before refusal')

    def test_leased_entry_calls_the_named_guard_itself(self):
        # C5r2: the leased refusal does not rest on the environment builder.
        admitted = self.admit()
        allowlisted = lambda: {k: os.environ[k] for k in gn._LAUNCH_ENVIRONMENT if k in os.environ}
        with patch.dict(os.environ, {'CARGO_BUILD_TARGET': 'x86_64-unknown-linux-musl'}), \
             patch.object(gn, '_cargo_environment', side_effect=allowlisted), \
             patch.object(gn.PrivateRecovery, 'acquire', autospec=True,
                          side_effect=gn.PrivateRecovery.acquire) as acquire:
            with self.assertRaisesRegex(gn.NeutralityError,
                                        'CARGO_BUILD_TARGET; unset it for the helper run'):
                gn.verify_task(admitted, exercise_nine=True)
            acquire.assert_not_called()

    def test_exact_nine_honest_cli_and_readback(self):
        self.assertEqual(self.cli('--exercise-nine').returncode, 0)
        path = next(self.evidence_root.glob('task-10/*/receipt.json'))
        receipt = json.loads(path.read_text())
        proof = receipt['nine_exercise']
        self.assertEqual(proof['paths'], list(gn.NINE_PATHS))
        self.assertEqual(proof['pre_removal_hashes'], proof['restored_hashes'])
        self.assertEqual(len(proof['before']['manifest']) - len(proof['deleted']['manifest']), 9)
        self.assertEqual(proof['before'], proof['after'])
        pins = self.pins()
        self.assertEqual(self.cli('--exercise-nine').returncode, 0)
        self.assertEqual(self.pins(), pins)
        self.assertEqual(list(Path(tempfile.gettempdir()).glob('neutrality-nine-*')), [])

    def test_missing_restore_and_changed_restore_fail_and_recover_nine(self):
        for selected in gn.NINE_PATHS:
            for changed in (False, True):
                with self.subTest(path=selected, changed=changed):
                    if (self.evidence_root / 'task-10').exists():
                        shutil.rmtree(self.evidence_root / 'task-10')
                    cargo = self.fake_bin / 'cargo'
                    script = self.generation_script()
                    line = next(line for line in script.splitlines() if str(self.repo / selected) in line)
                    if changed:
                        script = script.replace(line,
                            f'if [ ! -f "{self.repo / selected}" ]; then printf "wrong" > "{self.repo / selected}"; fi')
                    else:
                        script = script.replace(line, '')
                    cargo.write_text('#!/bin/sh\n' + script + '\n')
                    before = gn.filesystem_manifest(self.repo, self.scope()['output_rules'])
                    result = self.cli('--exercise-nine')
                    self.assertEqual(result.returncode, 2)
                    self.assertIn('not restored', result.stderr)
                    self.assertEqual(gn.filesystem_manifest(self.repo, self.scope()['output_rules']), before)
                    failure = json.loads(next(self.evidence_root.glob('task-10/*/nine-failure.json')).read_text())
                    self.assertIn('error', failure)
                    self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))

    def test_nine_failure_restores_nondefault_mode(self):
        # R5: a nondefault NON-executable pre-removal mode keeps the exact
        # full-mode restoration assertion. 0o750 would now be refused at task
        # admission as owner-execute drift, so the fixture cannot start there.
        selected = self.repo / gn.NINE_PATHS[0]
        selected.chmod(0o640)
        original = selected.read_bytes()
        script = self.generation_script()
        line = next(line for line in script.splitlines() if str(selected) in line)
        script = script.replace(line,
            f'if [ ! -f "{selected}" ]; then printf "wrong" > "{selected}"; fi')
        (self.fake_bin / 'cargo').write_text('#!/bin/sh\n' + script + '\n')
        result = self.cli('--exercise-nine')
        self.assertEqual(result.returncode, 2)
        self.assertIn('not restored', result.stderr)
        self.assertEqual(selected.read_bytes(), original)
        self.assertEqual(selected.stat().st_mode & 0o777, 0o640)

    def nine_chmod_on_create_script(self, selected):
        """Real generator seam: recreate one nine path executable.

        The chmod is applied only when the path was absent, i.e. during the
        nine regeneration, so no earlier regeneration boundary sees it. The
        recreated bytes stay byte-identical to the pre-removal content.
        """
        script = self.generation_script()
        line = next(line for line in script.splitlines() if str(self.repo / selected) in line)
        return script.replace(line,
            f'if [ ! -f "{self.repo / selected}" ]; then\n{line}\n'
            f'chmod 755 "{self.repo / selected}"\nfi')

    def test_nine_created_executable_mode_drift_refuses_and_restores_nine(self):
        # R5/A-R5-2: the nine regeneration is an actual generation boundary. Its
        # recreated files are absent from the general regeneration snapshot, so
        # drift there can only be seen against the modes captured before
        # deletion. Bytes match, the executable bit differs: refuse, emit no
        # receipt, and still restore every nine path byte- and mode-exactly.
        selected = gn.NINE_PATHS[3]
        saved = self.nine_originals()
        (self.fake_bin / 'cargo').write_text('#!/bin/sh\n'
            + self.nine_chmod_on_create_script(selected))
        result = self.cli('--exercise-nine')
        self.assertEqual(result.returncode, 2)
        self.assertIn('nine regeneration changed the generated executable mode', result.stderr)
        self.assert_nine_restored(saved)
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
        self.assertEqual(list(Path(tempfile.gettempdir()).glob('neutrality-nine-*')), [])

    def test_restore_failure_test_covers_every_nine_path(self):
        scripts = []
        cli = self.cli
        def capture(*args):
            scripts.append((self.fake_bin / 'cargo').read_text())
            return cli(*args)
        with patch.object(self, 'cli', side_effect=capture):
            self.test_missing_restore_and_changed_restore_fail_and_recover_nine()
        honest = '#!/bin/sh\n' + self.generation_script() + '\n'
        for path in gn.NINE_PATHS:
            line = next(line for line in honest.splitlines() if str(self.repo / path) in line)
            self.assertEqual(sum(line not in script.splitlines() for script in scripts), 2, path)

    def test_nine_regeneration_outside_write_fails_and_recovers(self):
        source = self.write('handwritten.txt', 'dirty operator bytes\n')
        first = self.repo / gn.NINE_PATHS[0]
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('#!/bin/sh\nif [ ! -f "' + str(first) + '" ]; then\n'
            + f'printf "bad" > "{source}"\nfi\n' + self.generation_script())
        baseline = gn.filesystem_manifest(self.repo, self.scope()['output_rules'])
        result = self.cli('--exercise-nine')
        self.assertEqual(result.returncode, 2)
        self.assertIn('outside generated domains', result.stderr)
        self.assertEqual(source.read_text(), 'dirty operator bytes\n')
        self.assertEqual(gn.filesystem_manifest(self.repo, self.scope()['output_rules']), baseline)
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
        state = json.loads(next(self.evidence_root.glob('task-10/*/nine-regen.state.json')).read_text())
        self.assertEqual(state['out_of_domain'], ['handwritten.txt'])

    def nine_originals(self):
        return {p: ((self.repo / p).read_bytes(), stat.S_IMODE((self.repo / p).stat().st_mode))
                for p in gn.NINE_PATHS}

    def assert_nine_restored(self, saved, exclude=()):
        for p, (data, mode) in saved.items():
            if p in exclude:
                continue
            path = self.repo / p
            self.assertTrue(path.is_file(), p)
            self.assertEqual(stat.S_IMODE(path.stat().st_mode), mode, p)
            self.assertEqual(path.read_bytes(), data, p)

    def test_recipe_edit_at_nine_regeneration_refuses_and_restores_nine(self):
        # R3: the nine boundary is an actual regeneration like any other.
        script = self.repo / 'tools/regen-all.sh'
        original = script.read_text()
        saved = self.nine_originals()
        admitted = self.admit()
        real = gn._regenerate

        def seam(a, directory, label, exclusions):
            if label == 'nine-regen':
                script.write_text(original + 'echo "changed-qualified-script"\n')
            return real(a, directory, label, exclusions)

        with patch.object(gn, '_regenerate', side_effect=seam):
            with self.assertRaisesRegex(gn.NeutralityError, 'current rule sources changed'):
                gn.verify_task(admitted, exercise_nine=True)
        self.assert_nine_restored(saved)
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
        self.assertEqual(list(Path(tempfile.gettempdir()).glob('neutrality-nine-*')), [])
        script.write_text(original)

    def test_nine_recovery_survives_pre_snapshot_unicode_error(self):
        # R7: the real pre-snapshot UTF-8 encode failure that appears after the
        # nine are removed must still unwind, not discard the saved copies.
        for p in gn.NINE_PATHS:
            (self.repo / p).chmod(0o640)
        saved = self.nine_originals()
        admitted = self.admit()
        real = gn._regenerate
        observed = []

        def seam(a, directory, label, exclusions):
            if label == 'nine-regen':
                absent = [p for p in gn.NINE_PATHS if not (self.repo / p).exists()]
                self.assertEqual(len(absent), 9)
                observed.append(True)
                os.symlink(b'\xff', os.fsencode(self.repo / 'undecodable-target'))
            return real(a, directory, label, exclusions)

        with patch.object(gn, '_regenerate', side_effect=seam):
            with self.assertRaises((gn.NeutralityError, UnicodeError)):
                gn.verify_task(admitted, exercise_nine=True)
        self.assertTrue(observed)
        self.assert_nine_restored(saved)
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
        self.assertEqual(list(Path(tempfile.gettempdir()).glob('neutrality-nine-*')), [])

    def test_preexisting_undecodable_target_still_refuses_before_deletion(self):
        # R7 keeps the unsupported-encoding refusal: it is refused before any
        # removal, so all nine originals survive untouched.
        os.symlink(b'\xff', os.fsencode(self.repo / 'undecodable-target'))
        saved = self.nine_originals()
        result = self.cli('--exercise-nine')
        self.assertEqual(result.returncode, 2)
        self.assert_nine_restored(saved)
        self.assertFalse(list(self.evidence_root.glob('task-10/*/nine-failure.json')))

    def test_generator_created_undecodable_target_still_restores_nine(self):
        # R7 retained control: an undecodable target the generator creates after
        # the nine are removed is aggregated as undecidable, and the nine are
        # still restored byte- and mode-exactly.
        saved = self.nine_originals()
        first = self.repo / gn.NINE_PATHS[0]
        (self.fake_bin / 'cargo').write_text('#!/bin/sh\n'
            + f'if [ ! -f "{first}" ]; then\n'
            + 'python3 -c \'import os; os.symlink(b"\\xff", "'
            + f'{self.repo}/undecodable-target")\'\nexit 7\nfi\n'
            + self.generation_script())
        result = self.cli('--exercise-nine')
        self.assertEqual(result.returncode, 2)
        self.assertIn('undecidable post-generation path', result.stderr)
        self.assert_nine_restored(saved)
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
        self.assertEqual(list(Path(tempfile.gettempdir()).glob('neutrality-nine-*')), [])

    def test_nine_recovery_survives_failure_record_write_error(self):
        # R7: restoration must not depend on the failure record being writable.
        selected = gn.NINE_PATHS[0]
        saved = self.nine_originals()
        script = self.generation_script()
        line = next(line for line in script.splitlines() if str(self.repo / selected) in line)
        script = script.replace(line,
            f'if [ ! -f "{self.repo / selected}" ]; then printf "wrong" > "{self.repo / selected}"; fi')
        (self.fake_bin / 'cargo').write_text('#!/bin/sh\n' + script + '\n')
        admitted = self.admit()
        original_create_only = gn.create_only

        def create_only(path, data):
            if path.name == 'nine-failure.json':
                raise OSError('injected failure-record write error')
            return original_create_only(path, data)

        with patch.object(gn, 'create_only', side_effect=create_only):
            with self.assertRaises(gn.NeutralityError) as failure:
                gn.verify_task(admitted, exercise_nine=True)
        message = str(failure.exception)
        self.assertIn('nine files not restored byte-identically', message)
        self.assertIn('failure evidence not recorded', message)
        self.assertIn('injected failure-record write error', message)
        self.assertIsInstance(failure.exception.__cause__, gn.NeutralityError)
        self.assert_nine_restored(saved)
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
        self.assertEqual(list(Path(tempfile.gettempdir()).glob('neutrality-nine-*')), [])

    def obstruct_nine(self, selected, seed=None):
        """Real generator seam: refuse one removal by leaving a directory there."""
        saved = {}
        for p in gn.NINE_PATHS:
            path = self.repo / p
            if path.is_dir():
                shutil.rmtree(path)
            if not path.exists():
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(gn._git(self.repo, 'show', self.base + ':' + p))
            path.chmod(0o640)
            saved[p] = (path.read_bytes(), 0o640)
        first = self.repo / gn.NINE_PATHS[0]
        unsafe = self.repo / selected
        (self.fake_bin / 'cargo').write_text('#!/bin/sh\n'
            + f'if [ ! -f "{first}" ]; then\nmkdir "{unsafe}"\n'
            + f'printf "synthetic obstruction" > "{unsafe}/marker"\nexit 7\nfi\n'
            + self.generation_script())
        return saved, unsafe

    def test_every_independently_safe_nine_sibling_is_restored(self):
        # R8: one unsafe destination refuses for itself; every other removed
        # path is still restored byte- and exact-mode-identically, in a
        # deterministic order independent of hash iteration.
        for index in (0, len(gn.NINE_PATHS) // 2, len(gn.NINE_PATHS) - 1):
            with self.subTest(index=index):
                if (self.evidence_root / 'task-10').exists():
                    shutil.rmtree(self.evidence_root / 'task-10')
                selected = gn.NINE_PATHS[index]
                saved, unsafe = self.obstruct_nine(selected)
                with patch.dict(os.environ, {'PYTHONHASHSEED': str(index + 1)}):
                    result = self.cli('--exercise-nine')
                self.assertEqual(result.returncode, 2)
                self.assertIn('unsafe nine recovery', result.stderr)
                self.assertTrue(unsafe.is_dir())
                self.assertEqual((unsafe / 'marker').read_text(), 'synthetic obstruction')
                self.assert_nine_restored(saved, exclude=(selected,))
                self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
                self.assertEqual(list(Path(tempfile.gettempdir()).glob('neutrality-nine-*')), [])
                shutil.rmtree(unsafe)

    def test_unsafe_sibling_and_unwritable_failure_record_both_report(self):
        # R8 + R7: an obstructed destination and an unwritable failure record
        # are both reported beside the original cause, and the remaining safe
        # siblings are still restored.
        selected = gn.NINE_PATHS[0]
        saved, unsafe = self.obstruct_nine(selected)
        admitted = self.admit()
        original_create_only = gn.create_only

        def create_only(path, data):
            if path.name == 'nine-failure.json':
                raise OSError('injected failure-record write error')
            return original_create_only(path, data)

        with patch.object(gn, 'create_only', side_effect=create_only):
            with self.assertRaises(gn.NeutralityError) as failure:
                gn.verify_task(admitted, exercise_nine=True)
        message = str(failure.exception)
        self.assertIn('unsafe nine recovery', message)
        self.assertIn('failure evidence not recorded', message)
        self.assertIn('injected failure-record write error', message)
        self.assertIsInstance(failure.exception.__cause__, gn.NeutralityError)
        self.assertTrue(unsafe.is_dir())
        self.assertEqual((unsafe / 'marker').read_text(), 'synthetic obstruction')
        self.assert_nine_restored(saved, exclude=(selected,))
        self.assertFalse(list(self.evidence_root.glob('task-10/*/receipt.json')))
        shutil.rmtree(unsafe)

    def test_historical_nine_hashes_and_missing_evidence_refuse(self):
        admitted = self.admit()
        receipt = gn.verify_task(admitted, True)
        for field in ('pre_removal_hashes', 'restored_hashes', 'deleted', 'paths'):
            changed = copy.deepcopy(receipt)
            changed['nine_exercise'][field] = {} if field.endswith('hashes') else []
            with self.subTest(field=field), self.assertRaises(gn.NeutralityError):
                gn._validate_task_receipt(changed, dict(admitted, exercise_nine=True), admitted['evidence'])
        changed = dict(receipt)
        del changed['nine_exercise']
        with self.assertRaisesRegex(gn.NeutralityError, 'missing nine'):
            gn._validate_task_receipt(changed, dict(admitted, exercise_nine=True), admitted['evidence'])

    def test_generated_dirt_refused_without_removal_or_regeneration(self):
        self.write(gn.NINE_PATHS[0], 'operator dirt\n')
        before = gn._working_files(self.repo, [])
        result = self.cli('--exercise-nine')
        self.assertEqual(result.returncode, 2)
        self.assertIn('dirty', result.stderr)
        self.assertEqual(gn._working_files(self.repo, []), before)
        self.assertFalse((self.evidence_root / 'task-10').exists())


class R7InterruptionTests(unittest.TestCase):
    """Owned synthetic topology only; watchdogs contain, never qualify a timeout."""
    nine_fixture = True
    setUp = ContextTaskTests.setUp
    git = ScopeTests.git
    write = ScopeTests.write
    commit = ScopeTests.commit
    scope = ScopeTests.scope
    put = ContextTaskTests.put
    admit = ContextTaskTests.admit
    build_admission = ContextTaskTests.build_admission
    configure_nine = ContextTaskTests.configure_nine
    generation_script = ContextTaskTests.generation_script
    nine_originals = NineTests.nine_originals
    assert_nine_restored = NineTests.assert_nine_restored

    def prepare(self):
        for index, p in enumerate(gn.NINE_PATHS):
            (self.repo / p).chmod((0o600, 0o640, 0o660)[index % 3])
        self.saved = self.nine_originals()
        self.scratch = self.evidence_root / 'scratch'
        self.scratch.mkdir(exist_ok=True)
        self.directory = self.evidence_root / 'nine-evidence'
        self.directory.mkdir(exist_ok=True)
        self.admitted = self.admit()
        # Keep every new test's scratch in its own fixture, including in-process runs.
        old = tempfile.tempdir
        tempfile.tempdir = str(self.scratch)
        self.addCleanup(setattr, tempfile, 'tempdir', old)

    def exercise(self):
        return gn._exercise_nine(self.admitted, self.directory, [], list(gn.NINE_PATHS), None)

    def assert_failure_cleanup(self, exclude=()):
        self.assert_nine_restored(self.saved, exclude)
        self.assertFalse(list(self.evidence_root.rglob('receipt.json')))
        self.assertEqual(list(self.scratch.iterdir()), [])

    def restore_fixture(self):
        for p, (data, mode) in self.saved.items():
            path = self.repo / p
            path.write_bytes(data)
            path.chmod(mode)
        for path in self.directory.iterdir():
            path.unlink()

    def test_r7_exception_matrix_and_secondary_failures(self):
        self.prepare()
        for cls in (ValueError, KeyError, TypeError, RuntimeError, SystemExit,
                    UnicodeError, OSError, gn.NeutralityError):
            for secondary in ('none', 'record', 'sibling', 'scratch'):
                with self.subTest(primary=cls.__name__, secondary=secondary):
                    self.restore_fixture()
                    primary = cls('primary sentinel')
                    real_create, real_chmod = gn.create_only, Path.chmod
                    real_cleanup = tempfile.TemporaryDirectory.cleanup
                    failed = []
                    def record(path, data):
                        if path.name == 'nine-failure.json' and secondary == 'record':
                            # Recording cannot run before restoration.
                            self.assert_nine_restored(self.saved)
                            raise KeyError('record sentinel')
                        return real_create(path, data)
                    def chmod(path, mode, *args, **kwargs):
                        if secondary == 'sibling' and path == self.repo / gn.NINE_PATHS[0]:
                            failed.append(True)
                            raise TypeError('restore sentinel')
                        return real_chmod(path, mode, *args, **kwargs)
                    def cleanup(td):
                        real_cleanup(td)
                        if secondary == 'scratch':
                            raise RuntimeError('scratch sentinel')
                    with patch.object(gn, '_regenerate', side_effect=primary), \
                         patch.object(gn, 'create_only', side_effect=record), \
                         patch.object(Path, 'chmod', chmod), \
                         patch.object(tempfile.TemporaryDirectory, 'cleanup', cleanup):
                        try:
                            self.exercise()
                        except BaseException as error:
                            caught = error
                        else:
                            self.fail('failure became success')
                    if (isinstance(primary, (gn.NeutralityError, OSError, UnicodeError))
                            and secondary != 'none'):
                        # C2: these three classes aggregate only when secondary
                        # notes exist; a lone one stays its own object, exactly as
                        # it did before R7 wrapped it unconditionally.
                        self.assertIsInstance(caught, gn.NeutralityError)
                        self.assertIsNot(caught, primary)
                        self.assertIn('primary sentinel', str(caught))
                    else:
                        self.assertIs(caught, primary)
                    if secondary != 'none':
                        self.assertIn({'record': 'record sentinel', 'sibling': 'restore sentinel',
                                       'scratch': 'scratch sentinel'}[secondary],
                                      str(caught) + str(getattr(caught, '__notes__', [])))
                    self.assert_failure_cleanup((gn.NINE_PATHS[0],) if failed else ())

    def test_r7_primary_scratch_disposal_failure_restores_modes(self):
        self.prepare()
        real = tempfile.TemporaryDirectory.cleanup
        primary = RuntimeError('primary disposal sentinel')
        calls = []
        def cleanup(td):
            real(td)
            calls.append(True)
            if len(calls) == 1:
                raise primary
        with patch.object(tempfile.TemporaryDirectory, 'cleanup', cleanup):
            with self.assertRaises(RuntimeError) as caught:
                self.exercise()
        self.assertIs(caught.exception, primary)
        self.assert_failure_cleanup()

    def test_r7_prior_callable_replay_and_main_thread_refusal(self):
        self.prepare()
        previous = signal.getsignal(signal.SIGINT)
        calls, observed = [], []
        def prior(signum, frame):
            calls.append(signum)
            observed.append(signal.getsignal(signal.SIGINT))
        signal.signal(signal.SIGINT, prior)
        real_run = gn.subprocess.run
        def run(argv, *args, **kwargs):
            if argv == ['bash', 'tools/regen-all.sh']:
                self.assertTrue(kwargs.get('start_new_session'))
                self.assertEqual(kwargs.get('stdin'), subprocess.DEVNULL)
                self.assertTrue(kwargs.get('capture_output'))
                self.assertNotIn('timeout', kwargs)
                result = real_run(argv, *args, **kwargs)
                signal.raise_signal(signal.SIGINT)
                return result
            return real_run(argv, *args, **kwargs)
        try:
            with patch.object(gn.subprocess, 'run', side_effect=run):
                with self.assertRaises(gn.NeutralityError):
                    self.exercise()
            self.assertEqual(calls, [signal.SIGINT])
            self.assertEqual(observed, [prior])
            self.assertIs(signal.getsignal(signal.SIGINT), prior)
            self.assert_failure_cleanup()
        finally:
            signal.signal(signal.SIGINT, previous)
        self.restore_fixture()
        # Handler installation in a worker must refuse, not proceed unguarded.
        import threading
        caught = []
        def worker():
            try:
                self.exercise()
            except BaseException as error:
                caught.append(error)
        thread = threading.Thread(target=worker)
        thread.start()
        thread.join(timeout=10)
        self.assertFalse(thread.is_alive())
        self.assertEqual(len(caught), 1)
        self.assertIsInstance(caught[0], gn.NeutralityError)
        self.assertIn('main thread', str(caught[0]))
        self.assert_failure_cleanup()

    def test_r7_signal_seams_and_nested_handler(self):
        self.prepare()
        previous = signal.getsignal(signal.SIGINT)
        for seam in ('0', 'between', 'L', '1', '2', 'dispose', '3', 'restore'):
            with self.subTest(seam=seam):
                self.restore_fixture()
                handlers, removed, launched = [], [], []
                real_unlink, real_manifest = Path.unlink, gn.filesystem_manifest
                real_run, real_cleanup, real_chmod = gn.subprocess.run, tempfile.TemporaryDirectory.cleanup, Path.chmod
                fired = []
                def fire():
                    if not fired:
                        handlers.append(signal.getsignal(signal.SIGINT))
                        fired.append(True)
                        signal.raise_signal(signal.SIGINT)
                def unlink(path, *args, **kwargs):
                    if str(path) in {str(self.repo / p) for p in gn.NINE_PATHS}:
                        real_unlink(path, *args, **kwargs)
                        removed.append(path)
                        if seam == 'between':
                            fire()  # after unlink, before removed.append on the old helper
                        return
                    return real_unlink(path, *args, **kwargs)
                def manifest(repo, rules):
                    result = real_manifest(repo, rules)
                    if seam == '2' and len(removed) == 9 and all((self.repo / p).exists() for p in gn.NINE_PATHS):
                        fire()
                    return result
                def run(argv, *args, **kwargs):
                    if argv == ['bash', 'tools/regen-all.sh']:
                        launched.append(True)
                        result = real_run(argv, *args, **kwargs)
                        if seam == '1':
                            fire()
                        return result
                    return real_run(argv, *args, **kwargs)
                def cleanup(td):
                    if seam == 'dispose':
                        fire()
                    real_cleanup(td)
                    if seam == '3':
                        fire()
                def chmod(path, mode, *args, **kwargs):
                    if seam == 'restore' and path == self.repo / sorted(gn.NINE_PATHS)[0]:
                        fire()
                    return real_chmod(path, mode, *args, **kwargs)
                real_unchanged = self.admitted['evidence'].unchanged
                real_write = Path.write_bytes
                def write(path, data):
                    result = real_write(path, data)
                    if seam == '0' and path.parent.name.startswith('neutrality-nine-') and path.name == '8':
                        fire()
                    return result
                def unchanged():
                    real_unchanged()
                    if seam == 'L':
                        fire()
                def regeneration(*args, **kwargs):
                    if seam == 'restore':
                        raise RuntimeError('restoration primary')
                    return real_regenerate(*args, **kwargs)
                real_regenerate = gn._regenerate
                with patch.object(Path, 'unlink', unlink), patch.object(gn, 'filesystem_manifest', manifest), \
                     patch.object(gn.subprocess, 'run', side_effect=run), \
                     patch.object(tempfile.TemporaryDirectory, 'cleanup', cleanup), \
                     patch.object(Path, 'chmod', chmod), patch.object(Path, 'write_bytes', write), \
                     patch.object(self.admitted['evidence'], 'unchanged', side_effect=unchanged), \
                     patch.object(gn, '_regenerate', side_effect=regeneration):
                    try:
                        self.exercise()
                    except BaseException as error:
                        caught = error
                    else:
                        self.fail('SIGINT became success')
                self.assertTrue(fired, 'instrumentation seam was not reached')
                self.assertTrue(all(callable(h) and h is not previous for h in handlers),
                                'SIGINT was not deferred inside restoration region')
                self.assertIs(signal.getsignal(signal.SIGINT), previous)
                self.assertIsInstance(caught, RuntimeError if seam == 'restore' else KeyboardInterrupt)
                if seam == 'restore':
                    self.assertIn('SIGINT', str(caught.__notes__))
                if seam == '0':
                    self.assertEqual(removed, [])
                if seam in ('0', 'L'):
                    self.assertEqual(launched, [])
                self.assert_failure_cleanup()

    def interrupt_at_private_acquisition_seam(self, seam):
        """One probe per fixture: a shared private namespace would let the first
        probe's residue decide the second's result instead of the code."""
        self.prepare()
        source = self.repo / 'bindings/python/README.md'
        relative = source.relative_to(self.repo).as_posix()
        dirt = b'operator edited tracked bytes\n'
        # Pre-existing out-of-domain dirt is a tracked-file modification, which is
        # what makes the acquisition publish a pending private snapshot at all.
        source.write_bytes(dirt)
        self.assertIn('M ' + relative,
                      self.git('status', '--porcelain', '--', relative), 'tracked dirt required')
        real_unchanged = self.admitted['evidence'].unchanged
        calls = []
        def unchanged():
            real_unchanged()
            calls.append(True)
            if len(calls) == (1 if seam == 'before-acquire' else 2):
                signal.raise_signal(signal.SIGINT)
        with patch.object(self.admitted['evidence'], 'unchanged', side_effect=unchanged):
            try:
                self.exercise()
            except BaseException as error:
                caught = error
            else:
                self.fail('SIGINT became success')
        self.assertTrue(calls, 'the private acquisition seam was not reached')
        self.assertIsInstance(caught, KeyboardInterrupt)
        self.assert_failure_cleanup()
        self.assertEqual(source.read_bytes(), dirt)
        with gn.PrivateRecovery(self.repo) as recovery:
            recovery.no_pending()

    def test_r7_interrupt_before_private_acquisition_leaves_no_pending(self):
        # P1: with pre-existing out-of-domain dirt the acquisition publishes a
        # pending private snapshot, and an interrupt detected after that
        # publication used to escape finalize and refuse every later helper run
        # (`no_pending`) until an operator ran `recovery-finalize`. The checkpoint
        # immediately before `recovery.acquire` is placed to detect the interrupt
        # before anything is published, so nothing is left pending.
        self.interrupt_at_private_acquisition_seam('before-acquire')

    def test_r7_interrupt_after_private_acquisition_leaves_no_pending(self):
        # After L' the published snapshot is not abandoned: the interrupt is held
        # until checkpoint 1, so the generator's normal run and its finalize
        # complete first. This seam must stay reachable and leave no residue.
        self.interrupt_at_private_acquisition_seam('after-acquire')

    def test_r7_arbitration_nonzero_ignore_and_unknown_disposition(self):
        self.prepare()
        prior = signal.getsignal(signal.SIGINT)
        real_run = gn.subprocess.run
        for disposition in (prior, signal.SIG_IGN):
            self.restore_fixture()
            signal.signal(signal.SIGINT, disposition)
            def run(argv, *args, **kwargs):
                if argv == ['bash', 'tools/regen-all.sh']:
                    result = real_run(argv, *args, **kwargs)
                    signal.raise_signal(signal.SIGINT)
                    result.returncode = 7
                    return result
                return real_run(argv, *args, **kwargs)
            try:
                with patch.object(gn.subprocess, 'run', side_effect=run):
                    try:
                        self.exercise()
                    except BaseException as error:
                        caught = error
                    else:
                        self.fail('nonzero generator succeeded')
                self.assertIsInstance(caught, gn.NeutralityError)
                self.assertIn('regeneration failed (exit 7); logs:', str(caught))
                self.assertIn('SIGINT', str(getattr(caught, '__notes__', [])))
                self.assertEqual(signal.getsignal(signal.SIGINT), disposition)
                self.assert_failure_cleanup()
            finally:
                signal.signal(signal.SIGINT, prior)
        # SIG_IGN plus interrupt alone still fails, but does not replay as KeyboardInterrupt.
        self.restore_fixture()
        signal.signal(signal.SIGINT, signal.SIG_IGN)
        def interrupt(argv, *args, **kwargs):
            result = real_run(argv, *args, **kwargs)
            if argv == ['bash', 'tools/regen-all.sh']:
                signal.raise_signal(signal.SIGINT)
            return result
        try:
            with patch.object(gn.subprocess, 'run', side_effect=interrupt):
                with self.assertRaises(gn.NeutralityError):
                    self.exercise()
            self.assert_failure_cleanup()
        finally:
            signal.signal(signal.SIGINT, prior)
        self.restore_fixture()
        unlinks = []
        real_unlink = Path.unlink
        def unlink(path, *args, **kwargs):
            if path in [self.repo / p for p in gn.NINE_PATHS]:
                unlinks.append(path)
            return real_unlink(path, *args, **kwargs)
        with patch.object(signal, 'getsignal', return_value=None), patch.object(Path, 'unlink', unlink):
            with self.assertRaises(gn.NeutralityError):
                self.exercise()
        self.assertEqual(unlinks, [])
        self.assert_failure_cleanup()

    def test_r7_ordinary_verify_non_raising_prior_disposition_still_fails(self):
        # C12 / amendment v2 item 4 on an ordinary verification (no nine
        # exercise): `_regenerate` has no checkpoint after the generator, so a
        # replay into SIG_IGN or a returning Python handler dropped the pending
        # interrupt and a receipt was emitted. It must fail without a receipt,
        # the prior disposition restored exactly.
        previous = signal.getsignal(signal.SIGINT)
        calls = []
        def returning(signum, frame):
            calls.append(signum)
        real_run = gn.subprocess.run
        for name, disposition in (('SIG_IGN', signal.SIG_IGN), ('returning handler', returning)):
            with self.subTest(prior=name):
                calls.clear()
                fired = []
                def run(argv, *args, **kwargs):
                    result = real_run(argv, *args, **kwargs)
                    if argv == ['bash', 'tools/regen-all.sh'] and not fired:
                        fired.append(True)
                        signal.raise_signal(signal.SIGINT)
                    return result
                admitted = self.admit()
                signal.signal(signal.SIGINT, disposition)
                try:
                    with patch.object(gn.subprocess, 'run', side_effect=run):
                        try:
                            receipt = gn.verify_task(admitted)
                        except BaseException as error:
                            outcome = error
                        else:
                            outcome = receipt
                    restored = signal.getsignal(signal.SIGINT)
                finally:
                    signal.signal(signal.SIGINT, previous)
                observed = dict(fired=bool(fired), failed=isinstance(outcome, gn.NeutralityError),
                                receipt=bool(list(self.evidence_root.glob('task-10/*/receipt.json'))),
                                restored=restored is disposition, handler_calls=list(calls))
                # Each subtest regenerates: never read back the previous request.
                for directory in self.evidence_root.glob('task-10/*'):
                    shutil.rmtree(directory)
                self.assertEqual(observed, dict(fired=True, failed=True, receipt=False, restored=True,
                                                handler_calls=[signal.SIGINT] if disposition is returning else []),
                                 f'outcome: {type(outcome).__name__}: {str(outcome)[:200]}')
                self.assertIn('SIGINT', str(outcome))

    def wait_file(self, path, process):
        deadline = time.monotonic() + 15
        while not path.exists() and process.poll() is None and time.monotonic() < deadline:
            time.sleep(.01)
        self.assertTrue(path.exists(), 'owned generator handshake not reached')
        return json.loads(path.read_text())

    def controlled_cli(self, *, target='pid', barrier=False, large=False):
        self.prepare()
        ready, release = self.evidence_root / 'ready.json', self.evidence_root / 'release'
        started = self.evidence_root / 'started'
        helper_pid = self.evidence_root / 'helper-pid'
        owned = self.evidence_root / 'owned.json'
        written = self.evidence_root / 'written'
        # The middle link waits synchronously; barrier variant deliberately exits
        # Bash while the owned descendant retains its inherited pipe write ends.
        child = self.evidence_root / 'child.py'
        child.write_text('import os,json,time,sys\nfrom pathlib import Path\n'
            + f'ready=Path({str(ready)!r}); release=Path({str(release)!r})\n'
            + f'owned=Path({str(owned)!r})\n'
            + 'tmp=owned.with_suffix(".partial")\n'
            + 'tmp.write_text(json.dumps(dict(pid=os.getpid(), group=os.getpgrp())))\n'
            + 'tmp.rename(owned)\n'
            + f'helper_pid=Path({str(helper_pid)!r})\n'
            + 'deadline=time.monotonic()+10\n'
            + 'while not helper_pid.exists() and time.monotonic()<deadline: time.sleep(.01)\n'
            + 'helper=int(helper_pid.read_text()); direct=os.getpid()\n'
            + 'for _ in range(12):\n'
            + '    text=Path(f"/proc/{direct}/status").read_text()\n'
            + '    parent=int(next(l.split()[1] for l in text.splitlines() if l.startswith("PPid:")))\n'
            + '    if parent==helper: break\n'
            + '    if parent<=1: raise RuntimeError("owned ancestry lost")\n'
            + '    direct=parent\n'
            + 'tmp=ready.with_suffix(".partial")\n'
            + 'tmp.write_text(json.dumps(dict(pid=os.getpid(), group=os.getpgrp(), direct=direct)))\n'
            + 'tmp.rename(ready)\n'
            + 'deadline=time.monotonic()+20\n'
            + 'while not release.exists() and time.monotonic()<deadline: time.sleep(.01)\n'
            + 'if not release.exists(): sys.exit(99)\n'
            + ("os.write(1,b'x'*(2*1024*1024)); os.write(2,b'y'*(2*1024*1024))\n" if large else '')
            + ''.join(f'Path({str(self.repo / p)!r}).write_bytes({data!r})\n' for p, (data, mode) in self.saved.items())
            + f'Path({str(written)!r}).write_text(str(time.monotonic()))\n')
        mid = self.evidence_root / 'middle.sh'
        mid.write_text('#!/bin/sh\n' + f'{sys.executable} "{child}"\n')
        mid.chmod(0o755)
        cargo = self.fake_bin / 'cargo'
        first = self.repo / gn.NINE_PATHS[0]
        cargo.write_text('#!/bin/sh\n' + f'if [ ! -f "{first}" ]; then\n'
            + (f'if [ ! -f "{started}" ]; then\ntouch "{started}"\n"{mid}" &\nfor attempt in $(seq 1 500); do\n[ -f "{ready}" ] && break\nsleep .01\ndone\nfi\nexit 0\n' if barrier else f'"{mid}"\nexit 0\n')
            + 'fi\n' + self.generation_script())
        env = dict(os.environ, TMPDIR=str(self.scratch), PYTHONDONTWRITEBYTECODE='1')
        argv = [sys.executable, str(self.helper), 'verify', '--task', '10',
                '--context', str(self.context_path), '--exercise-nine']
        # Capture into fixture files so even a failing containment path cannot
        # block the test parent's communicate() on an orphan's stdout.
        with (self.evidence_root / 'cli.out').open('wb') as out, \
             (self.evidence_root / 'cli.err').open('wb') as err:
            process = subprocess.Popen(argv, cwd=self.repo, env=env, stdout=out, stderr=err,
                                       start_new_session=True)
            groups = {process.pid}
            helper_pid.write_text(str(process.pid))
            try:
                info = self.wait_file(ready, process)
                groups.add(info['group'])
                self.assertEqual(len([p for p in gn.NINE_PATHS if not (self.repo / p).exists()]), 9)
                self.assertEqual(len(list(self.scratch.glob('neutrality-nine-*'))), 1)
                for _ in range(5):
                    if target == 'group':
                        os.killpg(process.pid, signal.SIGINT)
                    else:
                        os.kill(process.pid, signal.SIGINT)
                    time.sleep(.02)
                time.sleep(.15)
                if barrier:
                    # Distinguish the inherited-pipe EOF barrier from waiting
                    # on Bash: the direct child is already a zombie or gone.
                    deadline = time.monotonic() + 5
                    while time.monotonic() < deadline:
                        try:
                            status = Path(f'/proc/{info["direct"]}/stat').read_text().rsplit(')', 1)[1].split()[0]
                        except FileNotFoundError:
                            break
                        if status == 'Z':
                            break
                        time.sleep(.01)
                    else:
                        self.fail('direct Bash did not exit at the pipe barrier')
                self.assertIsNone(process.poll(), 'helper exited while a descendant can still write')
                self.assertFalse(written.exists())
                self.assertEqual(len(list(self.scratch.glob('neutrality-nine-*'))), 1)
                with self.assertRaisesRegex(gn.NeutralityError, 'busy private lease'):
                    with gn.PrivateRecovery(self.repo):
                        pass
                release.touch()
                rc = process.wait(timeout=15)
                finished = time.monotonic()
                self.assertTrue(written.exists())
                self.assertLess(float(written.read_text()), finished)
                text = (self.evidence_root / 'cli.err').read_text()
                # The real CLI mapping on this interpreter is signal termination,
                # not the shell's 128+SIGINT representation.
                self.assertEqual(rc, -signal.SIGINT, text)
                self.assertIn('KeyboardInterrupt', text)
                self.assert_failure_cleanup()
                with gn.PrivateRecovery(self.repo):
                    pass
                if large:
                    for stream, byte in (('stdout', b'x'), ('stderr', b'y')):
                        path = next(self.evidence_root.glob('task-10/*/nine-regen.' + stream))
                        self.assertEqual(path.read_bytes(), byte * (2 * 1024 * 1024))
                        state = json.loads(path.with_name('nine-regen.state.json').read_text())
                        self.assertEqual(state['command'][stream + '_sha256'], gn.sha256(path.read_bytes()))
            finally:
                release.touch()
                # Also account for a handshake published during a failed wait.
                if owned.exists():
                    groups.add(json.loads(owned.read_text())['group'])
                # Only groups created by this test and named by its handshake.
                for group in groups:
                    try:
                        os.killpg(group, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                process.wait(timeout=5)
                if owned.exists():
                    pid = json.loads(owned.read_text())['pid']
                    deadline = time.monotonic() + 5
                    while time.monotonic() < deadline:
                        try:
                            status = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[0]
                        except FileNotFoundError:
                            break
                        if status == 'Z':
                            break
                        time.sleep(.01)
                    else:
                        self.fail('owned descendant survived containment cleanup')

    def test_r7_real_sigint_pid_group_and_bursts(self):
        # Separate fresh fixture per targeting mode; no incomplete receipt reuse.
        self.controlled_cli(target='pid')

    def test_r7_real_sigint_group_and_bursts(self):
        self.controlled_cli(target='group')

    def test_r7_large_pipe_capture(self):
        self.controlled_cli(large=True)

    def test_r7_inherited_pipe_eof_barrier(self):
        self.controlled_cli(barrier=True)

    def test_r7_stalled_generator_has_no_timeout(self):
        self.controlled_cli()


class FinishTests(unittest.TestCase):
    nine_fixture = True
    configure_nine = ContextTaskTests.configure_nine
    # Reuse the synthetic repo/evidence fixture without duplicating inherited tests.
    setUp = ContextTaskTests.setUp
    git = ScopeTests.git
    write = ScopeTests.write
    commit = ScopeTests.commit
    scope = ScopeTests.scope
    put = ContextTaskTests.put
    admit = ContextTaskTests.admit
    build_admission = ContextTaskTests.build_admission
    cli = ContextTaskTests.cli
    pins = ContextTaskTests.pins
    generation_script = ContextTaskTests.generation_script

    def bind_task(self, task, context):
        self.put('context.json', context)
        admitted = gn.admit_context(str(self.context_path), task, self.repo, self.helper, self.test)
        receipt = gn.verify_task(admitted, exercise_nine=(task == '10'))
        accepted = self.commit()
        receipt_path = next(self.evidence_root.glob('task-' + task + '/*/receipt.json'))
        collection = json.loads(Path(context['dispatch_evidence'][task]['dispatch_snapshot_path']).read_text())
        # Pinned-producer shape: only a nonempty `recorded` set closes `pending`,
        # and `cleared` describes whole-wave completion rather than this task.
        collection.update(status='recorded', completed_at='2026-10-09T00:00:00.000Z',
                          record_outcome=dict(recorded=[int(task)], failed=[], cleared=True))
        collection['review_context'].update(episodes={}, emitted_reviews=['allowed'])
        collection_path = self.put('collection-' + task + '.json', collection)
        binding = dict(schema_version=1, **admitted['identity'],
            receipt_path=str(receipt_path), receipt_sha256=gn.sha256(receipt_path.read_bytes()),
            qualification_reference=context['qualifications'][task], accepted_code_commit=accepted,
            accepted_code_tree=self.git('rev-parse', accepted + '^{tree}'),
            native_collection_evidence_path=str(collection_path),
            native_collection_evidence_sha256=gn.sha256(collection_path.read_bytes()),
            code_delta_sha256=receipt['code_delta_sha256'],
            generated_after_manifest_sha256=receipt['after_manifest_sha256'])
        path = self.put('binding-' + task + '.json', binding)
        gn.validate_binding(binding, gn.Evidence(self.repo, self.evidence_root), 'synthetic-run', task)
        return binding, path

    def finish_context(self):
        self.write('bindings/python/README.md', 'task10 accepted handwritten change\n')
        b10, p10 = self.bind_task('10', self.context)
        context19 = copy.deepcopy(self.context)
        snapshot = json.loads((self.evidence_root / 'snapshot.json').read_text())
        snapshot.update(wave=1, key='key-1', wave_token='token-1', payload=[dict(id=19, files=['schemas/'])])
        snapshot['review_context'].update(base_sha=b10['accepted_code_commit'],
            tasks=[dict(task_id=19, repo=str(self.repo))])
        native = self.put('native-19.json', snapshot)
        pin = self.put('snapshot-19.json', snapshot)
        dispatch = dict(native_dispatch_path=str(native), dispatch_snapshot_path=str(pin),
                        dispatch_snapshot_sha256=gn.sha256(pin.read_bytes()))
        envelope = json.loads((self.evidence_root / 'prelaunch.json').read_text())
        envelope.update(task_id='19', wave=1, base_sha=b10['accepted_code_commit'], **dispatch)
        prelaunch = self.put('prelaunch-19.json', envelope)
        reference = dict(self.context['qualifications']['10'], binding_path=str(p10),
                         binding_sha256=gn.sha256(p10.read_bytes()))
        context19.update(dispatches={'19': str(native)}, dispatch_evidence={'19': dispatch},
                         prelaunch={'19': str(prelaunch)}, qualifications={'19': reference})
        self.write('schemas/current.yaml', 'task19 schema input\n')
        b19, p19 = self.bind_task('19', context19)
        context = copy.deepcopy(self.context)
        for name in ('dispatches', 'dispatch_evidence', 'prelaunch', 'qualifications'):
            context[name].update(context19[name])
        context.update(mode='finish', finish_head=b19['accepted_code_commit'],
            bindings={'10': str(p10), '19': str(p19)}, current_qualification=reference)
        return context, b10, b19

    def collection_binding(self, binding, record, name):
        path = self.put(name + '.json', record)
        return dict(binding, native_collection_evidence_path=str(path),
                    native_collection_evidence_sha256=gn.sha256(path.read_bytes()))

    def check_binding(self, binding, record, name):
        evidence = gn.Evidence(self.repo, self.evidence_root)
        gn.validate_binding(self.collection_binding(binding, record, name), evidence,
                            'synthetic-run', '10')

    def test_receipt_and_binding_name_the_edited_same_root_script(self):
        # C9: the receipt's rule for an edited, same-root recipe carries the
        # script that ran, and the binding rebuild agrees with it.
        script = self.repo / 'tools/regen-python.sh'
        prelaunch = [r for r in self.scope_at_base['output_rules'] if r['root'] == ROOT][0]
        script.write_text(script.read_text() + '# edited recipe, same root\n')
        current = gn.sha256(script.read_bytes())
        self.assertNotEqual(prelaunch['source_sha256'], current)
        self.build_admission()
        binding, _ = self.bind_task('10', self.context)
        receipt = json.loads(Path(binding['receipt_path']).read_bytes())
        self.assertEqual([r['root'] for r in receipt['output_rules']],
                         [r['root'] for r in self.scope_at_base['output_rules']])
        rule = [r for r in receipt['output_rules'] if r['root'] == ROOT][0]
        self.assertEqual(rule, dict(prelaunch, source_sha256=current))
        scope = gn.derive_scope(self.repo, binding['accepted_code_commit'])
        self.assertEqual(receipt['output_rules'],
                         gn.union_rules(gn.derive_scope(self.repo, self.base), scope))
        # bind_task already ran validate_binding; a receipt carrying the
        # prelaunch rule instead is refused by the binding.
        stale = dict(receipt, output_rules=[prelaunch if r['root'] == ROOT else r
                                            for r in receipt['output_rules']])
        path = self.put('stale-receipt.json', stale)
        with self.assertRaisesRegex(gn.NeutralityError, 'unbound historical output rules'):
            gn.validate_binding(dict(binding, receipt_path=str(path),
                                     receipt_sha256=gn.sha256(path.read_bytes())),
                                gn.Evidence(self.repo, self.evidence_root), 'synthetic-run', '10')

    def test_native_collection_success_shapes(self):
        # R4: identity equality never proves collection. The accepted shapes are
        # derived from the pinned producer (qualification 7e7e5f25): a nonempty
        # record_outcome.recorded membership, and the marker-cleared re-drive
        # that keeps its dispatched result. A retained historical record_error
        # beside a successful current outcome is legitimate; a note or timestamp
        # alone never is. A failed entry must carry a usable `id`, and a retained
        # result beside an ordinary recorded success must be coherent: the pinned
        # serializer writes the supplied record verbatim and inserts no default
        # `result`, so absence is ordinary success while a present value is
        # validated (null/nonobject is malformed, not another spelling of absence).
        self.write('bindings/python/README.md', 'accepted synthetic handwritten change\n')
        valid, _ = self.bind_task('10', self.context)
        snapshot = json.loads(Path(valid['dispatch_snapshot_path']).read_text())
        stamp = '2026-10-09T00:00:00.000Z'
        clear_note = ('marker already cleared \u2014 prior record transaction had completed; '
                      'record file finalized on re-drive')
        # Source-shaped retained result: the pinned consumer reads `result.tasks`
        # entries (item.digest or the item itself), `digest.task_id`, `digest.status`
        # and, only when it is an integer, `result.wave`. It requires no inner
        # run_id, review_context, key, attempt or token — those are outer record
        # fields, so inventing them in the fixture would fabricate a native shape.
        retained = lambda **kw: dict(dict(wave=0,
            tasks=[dict(digest=dict(task_id=10, status='done', summary='done'))]), **kw)
        digest = lambda **kw: dict(digest=dict(
            dict(task_id=10, status='done', summary='done'), **kw))
        failed_entry = lambda **kw: dict(status='failed', summary='unusable identity',
                                        blockers=[], **kw)
        accepted = {
            'recorded-requested-task-cleared-false': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(recorded=[10], cleared=False, failed=[
                    dict(id=19, status='failed', summary='other task failed', blockers=[])])),
            'recorded-decimal-string-task-id': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(recorded=['10'], failed=[], cleared=True)),
            'recorded-with-retained-record-error': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(recorded=[10], failed=[], cleared=True),
                record_error=dict(outcome='recorded-nothing', reason='an earlier attempt',
                                  recorded=[])),
            'recorded-without-retained-result': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(recorded=[10], failed=[], cleared=True)),
            'recorded-with-coherent-retained-result': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(recorded=[10], failed=[], cleared=True),
                result=retained()),
            'marker-cleared-redrive-with-retained-result': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(note=clear_note), result=retained()),
            # Consumer-compatible inner-wave absence: the pinned consumer refuses
            # a foreign result only when inner `wave` is an integer, so missing
            # wave is an acceptance boundary of that source — not proof of native
            # execution, an external writer or result authenticity.
            'recovery-consumer-compatible-missing-inner-wave': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(note=clear_note),
                result=dict(tasks=[dict(digest=dict(task_id=10, status='done', summary='done'))])),
        }
        for name, record in accepted.items():
            with self.subTest(shape=name):
                self.check_binding(valid, record, 'collection-' + name)
        refused = {
            'pending-snapshot-alias': snapshot,
            'pending-with-record-error': dict(snapshot, record_error=dict(
                outcome='recorded-nothing', reason='record-result recorded no tasks', recorded=[])),
            'recorded-other-task-only': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(recorded=[19], cleared=False, failed=[
                    dict(id=10, status='failed', summary='requested task failed', blockers=[])])),
            'recorded-duplicate-task-id': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(recorded=[10, '10'], failed=[], cleared=True)),
            'recorded-contradictory-failure': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(recorded=[10], cleared=True, failed=[
                    dict(id='10', status='failed', summary='contradiction', blockers=[])])),
            'recorded-backtick-not-an-id': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(recorded=[True], failed=[], cleared=True)),
            'recorded-failed-entry-missing-id': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(recorded=[10], cleared=True, failed=[failed_entry()])),
            'recorded-failed-entry-null-id': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(recorded=[10], cleared=True, failed=[failed_entry(id=None)])),
            'recorded-failed-entry-bool-id': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(recorded=[10], cleared=True, failed=[failed_entry(id=True)])),
            'recorded-failed-entry-container-id': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(recorded=[10], cleared=True, failed=[failed_entry(id=['19'])])),
            'recorded-failed-entry-wrong-alias': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(recorded=[10], cleared=True, failed=[failed_entry(task_id=19)])),
            'recorded-contradictory-retained-failure': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(recorded=[10], failed=[], cleared=True),
                result=retained(tasks=[digest(status='failed')])),
            'recorded-foreign-wave-retained-result': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(recorded=[10], failed=[], cleared=True),
                result=retained(wave=7)),
            'recorded-wrong-task-retained-result': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(recorded=[10], failed=[], cleared=True),
                result=retained(tasks=[dict(digest=dict(task_id=19, status='done'))])),
            'recorded-duplicate-requested-retained-digests': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(recorded=[10], failed=[], cleared=True),
                result=retained(tasks=[digest(), digest(task_id='10')])),
            'recorded-null-retained-result': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(recorded=[10], failed=[], cleared=True), result=None),
            'recorded-nonobject-retained-result': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(recorded=[10], failed=[], cleared=True), result=['done']),
            'recovery-contradictory-inner-run-id': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(note=clear_note),
                result=retained(run_id='other-run')),
            'recovery-contradictory-inner-base': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(note=clear_note),
                result=retained(review_context=dict(base_sha='0' * 40))),
            'recorded-without-completion-time': dict(snapshot, status='recorded',
                record_outcome=dict(recorded=[10], failed=[], cleared=True)),
            'note-only-recovery': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(note=clear_note)),
            'recovery-note-without-result-task': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(note=clear_note),
                result=dict(run_id='synthetic-run', wave=0,
                            review_context=dict(base_sha=self.base), tasks=[])),
            'recovery-failed-requested-digest': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(note=clear_note),
                result=retained(tasks=[dict(digest=dict(task_id=10, status='failed'))])),
            'recovery-duplicate-requested-digest': dict(snapshot, status='recorded',
                completed_at=stamp, record_outcome=dict(note=clear_note),
                result=retained(tasks=[dict(digest=dict(task_id=10, status='done')),
                                       dict(digest=dict(task_id='10', status='done'))])),
            'recovery-wrong-task-digest': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(note=clear_note),
                result=retained(tasks=[dict(digest=dict(task_id=19, status='done'))])),
            'recovery-wrong-wave-result': dict(snapshot, status='recorded', completed_at=stamp,
                record_outcome=dict(note=clear_note), result=retained(wave=7)),
        }
        for name, record in refused.items():
            with self.subTest(shape=name), self.assertRaises(gn.NeutralityError):
                self.check_binding(valid, record, 'collection-' + name)
        for field in ('key', 'attempt', 'wave_token'):
            with self.subTest(outer=field), self.assertRaisesRegex(gn.NeutralityError,
                                                                  'wrong collection'):
                self.check_binding(valid, dict(accepted['recorded-decimal-string-task-id'],
                                               **{field: 'different'}), 'collection-wrong-' + field)

    def test_finish_refuses_preexisting_generated_executable_mode_drift(self):
        # R5: finish reproduces a committed revision exactly, so a pre-existing
        # executable-mode drift from finish_head refuses before generation and
        # the unchanged tree remains a positive control.
        context, _, _ = self.finish_context()
        target = self.repo / (ROOT + '/tracked.py')
        self.assertFalse(gn._git_executable(target.stat().st_mode))
        target.chmod(0o755)
        try:
            with self.assertRaisesRegex(gn.NeutralityError, 'executable mode'):
                self.admit(context)
            self.assertFalse(list(self.evidence_root.glob('finish/*/receipt.json')))
        finally:
            target.chmod(0o644)
        receipt = gn.verify_finish(self.admit(context))
        self.assertEqual(receipt['after_manifest_sha256'], receipt['before_manifest_sha256'])

    def test_accepted_generated_mode_drift_refuses_without_recorded_delta(self):
        # R5/A-R5-3: a generated-mode change carried by the accepted commit must
        # be refused at the historical accepted-code binding, whatever the task
        # delta records.
        self.write('bindings/python/README.md', 'accepted synthetic handwritten change\n')
        valid, _ = self.bind_task('10', self.context)
        target = self.repo / (ROOT + '/tracked.py')
        target.chmod(0o755)
        self.git('add', '--', ROOT + '/tracked.py')
        self.git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                 'commit', '-qm', 'accepted generated executable-mode drift')
        drifted = self.git('rev-parse', 'HEAD')
        binding = dict(valid, accepted_code_commit=drifted,
                       accepted_code_tree=self.git('rev-parse', drifted + '^{tree}'))
        with self.assertRaisesRegex(gn.NeutralityError, 'executable-mode drift'):
            gn.validate_binding(binding, gn.Evidence(self.repo, self.evidence_root),
                                'synthetic-run', '10')

    def test_accepted_generated_mode_drift_refuses_even_when_truthfully_recorded(self):
        # R5/A-R5-3: accurate accounting is not neutrality. A truthful task code
        # delta that names the generated path and its 0o644 -> 0o755 owner-execute
        # change still cannot bind the accepted commit; the accepted binding is
        # refused outright.
        self.write('bindings/python/README.md', 'accepted synthetic handwritten change\n')
        valid, _ = self.bind_task('10', self.context)
        target = ROOT + '/tracked.py'
        (self.repo / target).chmod(0o755)
        self.git('add', '--', target)
        self.git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                 'commit', '-qm', 'accepted generated executable-mode drift')
        drifted = self.git('rev-parse', 'HEAD')
        entries = gn._git_entries(self.repo, drifted)
        truthful = gn._code_delta(self.repo, valid['base_sha'], {
            p: gn._git_file_state(self.repo, entry[0], entry[2]) for p, entry in entries.items()})
        recorded = next(c for c in truthful if c['path'] == target)
        self.assertEqual((recorded['before_mode'], recorded['after_mode']), (0o644, 0o755))
        self.assertEqual(recorded['before_sha256'], recorded['after_sha256'])
        receipt = json.loads(Path(valid['receipt_path']).read_text())
        receipt['code_delta'] = truthful
        receipt['code_delta_sha256'] = gn.canonical_digest(truthful)
        rewritten = self.put('receipt-recorded-drift.json', gn._encoded(receipt))
        binding = dict(valid, accepted_code_commit=drifted,
                       accepted_code_tree=self.git('rev-parse', drifted + '^{tree}'),
                       receipt_path=str(rewritten),
                       receipt_sha256=gn.sha256(rewritten.read_bytes()),
                       code_delta_sha256=receipt['code_delta_sha256'])
        with self.assertRaisesRegex(gn.NeutralityError, 'executable-mode drift'):
            gn.validate_binding(binding, gn.Evidence(self.repo, self.evidence_root),
                                'synthetic-run', '10')

    def test_binding_accepts_newly_authored_helper_absent_at_base(self):
        self.git('rm', '--cached', 'tools/check-generated-neutrality.py', 'tools/test_check_generated_neutrality.py')
        self.git('-c', 'user.name=Test', '-c', 'user.email=test@example.invalid',
                 'commit', '-qm', 'base without future helpers')
        base = self.git('rev-parse', 'HEAD')
        snapshot = json.loads((self.evidence_root / 'snapshot.json').read_text())
        snapshot['review_context']['base_sha'] = base
        pin = self.put('snapshot.json', snapshot)
        self.dispatch['dispatch_snapshot_sha256'] = gn.sha256(pin.read_bytes())
        self.context['dispatch_evidence']['10'] = self.dispatch
        envelope = json.loads((self.evidence_root / 'prelaunch.json').read_text())
        envelope.update(base_sha=base, **self.dispatch)
        self.put('prelaunch.json', envelope)
        qualified = json.loads((self.evidence_root / 'qualification.json').read_text())
        qualified['source_identity']['base_sha'] = base
        path = self.put('qualification.json', qualified)
        self.context['qualifications']['10']['source_identity'] = qualified['source_identity']
        self.context['qualifications']['10']['qualification_receipt_sha256'] = gn.sha256(path.read_bytes())
        binding, _ = self.bind_task('10', self.context)
        self.assertNotIn('tools/check-generated-neutrality.py', gn._git_entries(self.repo, base))
        self.assertIn('tools/check-generated-neutrality.py', gn._git_entries(self.repo, binding['accepted_code_commit']))

    def test_finish_nine_uses_current_bytes_and_explicit_rename_amendment(self):
        context, _, _ = self.finish_context()
        old, new = gn.NINE_PATHS[-1], 'bindings/openapi/generated/pages/renamed.schema.json'
        self.git('mv', old, new)
        self.write(new, 'current renamed bytes\n')
        context['finish_head'] = self.commit()
        script = self.generation_script().replace(old, new).replace('original language 8', 'current renamed bytes')
        (self.fake_bin / 'cargo').write_text('#!/bin/sh\n' + script)
        result = self.cli_finish(context, '--exercise-nine')
        self.assertEqual(result.returncode, 2)
        self.assertIn(old, result.stderr)
        self.assertIn('explicit recorded amendment', result.stderr)
        self.assertFalse((self.repo / old).exists())
        amendment = dict(schema_version=1, run_id=context['run_id'], repo=str(self.repo),
            task_id='10', finish_head=context['finish_head'], reason='parent-authorized synthetic rename',
            renames=[{'from': old, 'to': new}])
        path = self.put('nine-amendment.json', amendment)
        context['nine_amendment'] = dict(path=str(path), sha256=gn.sha256(path.read_bytes()))
        self.assertEqual(self.cli_finish(context, '--exercise-nine').returncode, 0)
        receipt = next(json.loads(p.read_text()) for p in self.evidence_root.glob('finish/*/receipt.json')
                       if json.loads(p.read_text()).get('exercise_nine'))
        proof = receipt['nine_exercise']
        self.assertEqual(proof['pre_removal_hashes'][new], gn.sha256(b'current renamed bytes\n'))
        self.assertEqual(proof['pre_removal_hashes'], proof['restored_hashes'])
        self.assertEqual(proof['amendment'], amendment)
        context['nine_amendment']['sha256'] = '0' * 64
        self.assertEqual(self.cli_finish(context, '--exercise-nine').returncode, 2)

    def cli_finish(self, context, *args):
        self.put('context.json', context)
        return self.cli(*args)

    def test_finish_nine_created_executable_mode_drift_refuses_and_restores_nine(self):
        # R5/A-R5-2 in finish mode: the pre-exercise finish admission compares
        # modes against finish_head, so it cannot see drift the nine regeneration
        # itself creates. The nine boundary must refuse it, emit no finish
        # receipt and restore every nine path byte- and mode-exactly.
        context, _, _ = self.finish_context()
        selected = gn.NINE_PATHS[5]
        saved = {p: ((self.repo / p).read_bytes(), stat.S_IMODE((self.repo / p).stat().st_mode))
                 for p in gn.NINE_PATHS}
        script = self.generation_script()
        line = next(line for line in script.splitlines() if str(self.repo / selected) in line)
        (self.fake_bin / 'cargo').write_text('#!/bin/sh\n' + script.replace(line,
            f'if [ ! -f "{self.repo / selected}" ]; then\n{line}\n'
            f'chmod 755 "{self.repo / selected}"\nfi'))
        result = self.cli_finish(context, '--exercise-nine')
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertIn('nine regeneration changed the generated executable mode', result.stderr)
        for p, (data, mode) in saved.items():
            self.assertEqual((self.repo / p).read_bytes(), data, p)
            self.assertEqual(stat.S_IMODE((self.repo / p).stat().st_mode), mode, p)
        self.assertFalse(list(self.evidence_root.glob('finish/*/receipt.json')))
        self.assertEqual(list(Path(tempfile.gettempdir()).glob('neutrality-nine-*')), [])

    def test_both_modes_capture_outside_creates_modifies_deletes_and_preserve_dirt(self):
        for mode in ('task', 'finish'):
            context = self.context if mode == 'task' else self.finish_context()[0]
            for action in ('create', 'modify', 'delete'):
                with self.subTest(mode=mode, action=action):
                    root = self.evidence_root / ('task-10' if mode == 'task' else 'finish')
                    if root.exists():
                        shutil.rmtree(root)
                    path = self.repo / 'handwritten.ignored'
                    self.write('.gitignore', '*.ignored\n')
                    if action == 'create':
                        path.unlink(missing_ok=True)
                        command = f'printf "bad" > "{path}"\n'
                    else:
                        path.write_text('operator dirt\n')
                        command = f'rm "{path}"\n' if action == 'delete' else f'printf "bad" > "{path}"\n'
                    (self.fake_bin / 'cargo').write_text('#!/bin/sh\n' + self.generation_script() + command)
                    self.put('context.json', context)
                    admitted = self.admit()
                    with self.assertRaisesRegex(gn.NeutralityError, 'outside generated domains.*handwritten.ignored'):
                        (gn.verify_task if mode == 'task' else gn.verify_finish)(admitted)
                    if action != 'create':
                        self.assertEqual(path.read_text(), 'operator dirt\n')
                    state = json.loads(next(root.glob('*/regen-0.state.json')).read_text())
                    self.assertIn('handwritten.ignored', state['out_of_domain'])
                    self.assertFalse(list(root.glob('*/receipt.json')))
            # Reset only this synthetic generator/source dirt before making bindings.
            path.unlink(missing_ok=True)
            (self.repo / '.gitignore').unlink(missing_ok=True)
            (self.fake_bin / 'cargo').write_text('#!/bin/sh\n' + self.generation_script())
            if mode == 'task':
                shutil.rmtree(self.evidence_root / 'task-10')

    def test_dirty_deleted_and_symlink_sources_are_preserved(self):
        for mode in ('task', 'finish'):
            context = self.context if mode == 'task' else self.finish_context()[0]
            tracked = self.repo / 'bindings/python/README.md'
            original = tracked.read_bytes()
            for kind in ('deleted', 'symlink'):
                with self.subTest(mode=mode, kind=kind):
                    root = self.evidence_root / ('task-10' if mode == 'task' else 'finish')
                    if root.exists():
                        shutil.rmtree(root)
                    tracked.unlink()
                    if kind == 'symlink':
                        tracked.symlink_to('missing-operator-target')
                    cargo = self.fake_bin / 'cargo'
                    cargo.write_text('#!/bin/sh\n' + self.generation_script()
                        + f'rm -f "{tracked}"\nprintf "bad" > "{tracked}"\n')
                    self.put('context.json', context)
                    with self.assertRaisesRegex(gn.NeutralityError, 'outside generated domains'):
                        (gn.verify_task if mode == 'task' else gn.verify_finish)(self.admit())
                    if kind == 'symlink':
                        self.assertEqual(os.readlink(tracked), 'missing-operator-target')
                        tracked.unlink()
                    else:
                        self.assertFalse(tracked.exists())
                    tracked.write_bytes(original)
            cargo.write_text('#!/bin/sh\n' + self.generation_script())
            if mode == 'task':
                shutil.rmtree(self.evidence_root / 'task-10')

    def test_cargo_exclusions_ignore_environment_and_require_tracked_declaration(self):
        self.write('Cargo.toml', '[workspace]\n')
        cargo = self.fake_bin / 'cargo'
        target = self.repo / 'target'
        cargo.write_text('#!/bin/sh\n[ -z "$CARGO_TARGET_DIR$CARGO_BUILD_TARGET_DIR" ] || exit 91\n'
            + "printf '%s' '" + json.dumps({'target_directory': str(target)}) + "'\n")
        with patch.dict(os.environ, {'CARGO_TARGET_DIR': str(self.repo / 'tools'),
                                   'CARGO_BUILD_TARGET_DIR': str(self.repo / ROOT)}):
            self.assertEqual(gn._build_exclusions(self.repo), [target])
        target = self.repo / 'declared-build'
        cargo.write_text("#!/bin/sh\nprintf '%s' '"
                         + json.dumps({'target_directory': str(target)}) + "'\n")
        with self.assertRaises(gn.NeutralityError):
            gn._build_exclusions(self.repo)
        config = self.write('.cargo/config.toml', '[build]\ntarget-dir = "declared-build"\n')
        with self.assertRaises(gn.NeutralityError):
            gn._build_exclusions(self.repo)
        self.git('add', '.cargo/config.toml')
        self.git('-c', 'user.name=Test', '-c', 'user.email=t@example.invalid',
                 'commit', '-qm', 'declared target')
        self.assertEqual(gn._build_exclusions(self.repo), [target])
        config.write_text('[build]\ntarget-dir = "tools"\n')
        with self.assertRaises(gn.NeutralityError):
            gn._build_exclusions(self.repo)

    def test_undeclared_and_untracked_cargo_target_exclusions_fail_closed(self):
        self.write('Cargo.toml', '[workspace]\n')
        target = self.repo / 'scratch'
        cargo = self.fake_bin / 'cargo'
        cargo.write_text("#!/bin/sh\nprintf '%s' '"
                         + json.dumps({'target_directory': str(target)}) + "'\n")
        with self.assertRaises(gn.NeutralityError):
            gn._build_exclusions(self.repo)
        self.write('.cargo/config', '[build]\ntarget-dir = "scratch"\n')
        with self.assertRaises(gn.NeutralityError):
            gn._build_exclusions(self.repo)

    def test_regeneration_drops_cargo_target_environment(self):
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('#!/bin/sh\n[ -z "$CARGO_TARGET_DIR$CARGO_BUILD_TARGET_DIR" ] || exit 91\n'
                         + self.generation_script())
        with patch.dict(os.environ, {'CARGO_TARGET_DIR': str(self.repo / 'tools'),
                                   'CARGO_BUILD_TARGET_DIR': str(self.repo / ROOT)}):
            self.assertEqual(gn.verify_task(self.admit())['commands'][0]['exit'], 0)

    def test_source_derived_build_exclusion_and_ambiguous_exclusions(self):
        for mode in ('task', 'finish'):
            context = self.context if mode == 'task' else self.finish_context()[0]
            self.write('Cargo.toml', '[workspace]\n')
            self.write('scratch/not-build.txt', 'handwritten\n')
            target = self.repo / 'cargo-configured-output'
            self.write('.cargo/config.toml', '[build]\ntarget-dir = "cargo-configured-output"\n')
            self.git('add', '.cargo/config.toml')
            if self.git('diff', '--cached', '--name-only'):
                self.git('-c', 'user.name=Test', '-c', 'user.email=t@example.invalid',
                         'commit', '-qm', 'source declared build target')
            metadata = json.dumps({'target_directory': str(target)})
            cargo = self.fake_bin / 'cargo'
            cargo.write_text('#!/bin/sh\nif [ "$1" = metadata ]; then\n'
                + "printf '%s' '" + metadata + "'\nexit 0\nfi\n"
                + self.generation_script() + f'mkdir -p "{target}"\nprintf "build" > "{target / "artifact"}"\n')
            self.put('context.json', context)
            receipt = (gn.verify_task if mode == 'task' else gn.verify_finish)(self.admit())
            for state in receipt['working_state']:
                self.assertNotIn('cargo-configured-output/artifact', state['after'])
                self.assertIn('scratch/not-build.txt', state['after'])
            for bad in (self.repo, self.repo / 'tools', self.repo / ROOT):
                cargo.write_text('#!/bin/sh\nprintf \'%s\' \''
                                 + json.dumps({'target_directory': str(bad)}) + "'\n")
                with self.assertRaisesRegex(gn.NeutralityError, 'exclusion'):
                    (gn.verify_task if mode == 'task' else gn.verify_finish)(self.admit())
            (self.repo / 'Cargo.toml').unlink()
            (self.repo / 'scratch/not-build.txt').unlink()
            shutil.rmtree(target)
            cargo.write_text('#!/bin/sh\n' + self.generation_script())
            if mode == 'task':
                shutil.rmtree(self.evidence_root / 'task-10')

    def test_finish_missing_stale_unbound_and_mismatched_binding(self):
        context, b10, _ = self.finish_context()
        for field, value in (('run_id', 'wrong-run'), ('task_id', '19'), ('wave', 9),
                             ('accepted_code_commit', self.base), ('accepted_code_tree', '0' * 40),
                             ('code_delta_sha256', '0' * 64), ('generated_after_manifest_sha256', '0' * 64),
                             ('receipt_sha256', '0' * 64)):
            with self.subTest(field=field):
                changed = dict(b10, **{field: value})
                self.put('binding-10.json', changed)
                with self.assertRaises(gn.NeutralityError):
                    self.admit(context)
        self.put('binding-10.json', b10)
        for field in ('10', '19'):
            changed = copy.deepcopy(context)
            del changed['bindings'][field]
            with self.subTest(missing=field), self.assertRaises(gn.NeutralityError):
                self.admit(changed)
        changed = dict(context, finish_head=self.base)
        with self.assertRaisesRegex(gn.NeutralityError, 'stale finish_head'):
            self.admit(changed)
        Path(context['bindings']['19']).unlink()
        with self.assertRaises(OSError):
            self.admit(context)

    def test_bound_before_after_delta_and_unaccounted_task_changes(self):
        context, b10, _ = self.finish_context()
        path = Path(b10['receipt_path'])
        original = json.loads(path.read_text())
        for alteration in ('before', 'after', 'omitted'):
            receipt = copy.deepcopy(original)
            if alteration == 'omitted':
                receipt['code_delta'] = []
            else:
                receipt['code_delta'][0][alteration + '_sha256'] = '0' * 64
            receipt['code_delta_sha256'] = gn.canonical_digest(receipt['code_delta'])
            self.put(path.relative_to(self.evidence_root).as_posix(), receipt)
            binding = dict(b10, receipt_sha256=gn.sha256(path.read_bytes()),
                           code_delta_sha256=receipt['code_delta_sha256'])
            with self.subTest(alteration=alteration), self.assertRaisesRegex(
                    gn.NeutralityError, 'code delta|tested bytes|unaccounted task change'):
                gn.validate_binding(binding, gn.Evidence(self.repo, self.evidence_root), 'synthetic-run', '10')

    def test_historical_generated_manifest_and_native_collection_mismatch(self):
        _, b10, _ = self.finish_context()
        receipt_path = Path(b10['receipt_path'])
        receipt = json.loads(receipt_path.read_text())
        for prefix in ('before', 'after'):
            changed = copy.deepcopy(receipt)
            changed[prefix + '_manifest'] = []
            receipt_path.write_bytes(gn._encoded(changed))
            binding = dict(b10, receipt_sha256=gn.sha256(receipt_path.read_bytes()))
            with self.subTest(prefix=prefix), self.assertRaises(gn.NeutralityError):
                gn.validate_binding(binding, gn.Evidence(self.repo, self.evidence_root), 'synthetic-run', '10')
        receipt_path.write_bytes(gn._encoded(receipt))
        collection_path = Path(b10['native_collection_evidence_path'])
        original = json.loads(collection_path.read_text())
        for field in ('run_id', 'wave', 'key', 'attempt', 'wave_token'):
            changed = dict(original, **{field: 'wrong'})
            collection_path.write_bytes(gn._encoded(changed))
            binding = dict(b10, native_collection_evidence_sha256=gn.sha256(collection_path.read_bytes()))
            with self.subTest(field=field), self.assertRaises(gn.NeutralityError):
                gn.validate_binding(binding, gn.Evidence(self.repo, self.evidence_root), 'synthetic-run', '10')

    def test_current_tree_dirty_untracked_ignored_and_nonreproducible_refused(self):
        context, _, _ = self.finish_context()
        for path in (ROOT + '/tracked.py', ROOT + '/new.py', ROOT + '/hidden.ignored'):
            self.write(path, 'unqualified generated bytes\n')
            with self.subTest(path=path), self.assertRaisesRegex(gn.NeutralityError, 'generated manifest'):
                self.admit(context)
            (self.repo / path).unlink()
            self.output.write_text('generated bytes\n')
        self.write('.gitignore', '*.ignored\n')
        self.write(ROOT + '/hidden.ignored', 'ignored generated bytes\n')
        with self.assertRaisesRegex(gn.NeutralityError, 'generated manifest'):
            self.admit(context)
        (self.repo / (ROOT + '/hidden.ignored')).unlink()
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('#!/bin/sh\nprintf "not reproducible" > "' + str(self.output) + '"\n')
        with self.assertRaisesRegex(gn.NeutralityError, 'not reproducible'):
            gn.verify_finish(self.admit(context))
        self.assertFalse(list(self.evidence_root.glob('finish/*/receipt.json')))

    def requalify_current(self, context, *, linked):
        prior = context['current_qualification']
        source = dict(base_sha=context['finish_head'], helper_sha256=gn.sha256(self.helper.read_bytes()),
                      test_sha256=gn.sha256(self.test.read_bytes()), scope_sources=self.scope()['scope_sources'])
        qualified = dict(helper_path=str(self.helper), helper_sha256=source['helper_sha256'],
                         test_path=str(self.test), test_sha256=source['test_sha256'], source_identity=source)
        result = subprocess.run(['python3', 'tools/test_check_generated_neutrality.py'],
                                cwd=self.repo, capture_output=True)
        self.assertEqual(result.returncode, 0)
        stdout = self.put('requalified.stdout', result.stdout)
        stderr = self.put('requalified.stderr', result.stderr)
        command = dict(argv=['python3', 'tools/test_check_generated_neutrality.py'], cwd=str(self.repo), exit=0,
                       stdout_path=str(stdout), stdout_sha256=gn.sha256(result.stdout),
                       stderr_path=str(stderr), stderr_sha256=gn.sha256(result.stderr))
        receipt = dict(schema_version=1, run_id='synthetic-run', repo=str(self.repo), task_id='10',
                       commands=[command], **qualified)
        if linked:
            receipt['prior_qualification_reference'] = copy.deepcopy(prior)
        path = self.put('requalification.json', receipt)
        context['current_qualification'] = dict(**qualified, qualification_receipt_path=str(path),
                                               qualification_receipt_sha256=gn.sha256(path.read_bytes()))
        return receipt

    def test_later_helper_test_source_changes_require_parent_linked_requalification(self):
        context, _, _ = self.finish_context()
        original = {p: p.read_bytes() for p in (self.helper, self.test, self.repo / MAIN)}
        for path in original:
            path.write_bytes(original[path] + b'\n')
            context['finish_head'] = self.commit()
            with self.subTest(path=path), self.assertRaisesRegex(gn.NeutralityError, 'unqualified|scope_sources'):
                self.admit(context)
            path.write_bytes(original[path])
            context['finish_head'] = self.commit()
        self.helper.write_bytes(original[self.helper] + b'\n# later qualified helper\n')
        self.test.write_bytes(original[self.test] + b'\n# later qualified tests\n')
        (self.repo / MAIN).write_bytes(original[self.repo / MAIN] + b'\n')
        context['finish_head'] = self.commit()
        saved = copy.deepcopy(context)
        self.requalify_current(context, linked=False)
        with self.assertRaisesRegex(gn.NeutralityError, 'missing linked requalification'):
            self.admit(context)
        context = saved
        receipt = self.requalify_current(context, linked=True)
        gn.verify_finish(self.admit(context))
        receipt['prior_qualification_reference']['qualification_receipt_sha256'] = '0' * 64
        path = self.put('requalification.json', receipt)
        context['current_qualification']['qualification_receipt_sha256'] = gn.sha256(path.read_bytes())
        with self.assertRaisesRegex(gn.NeutralityError, 'digest mismatch'):
            self.admit(context)

    def test_missing_current_qualification_and_wrong_historical_link(self):
        context, _, _ = self.finish_context()
        changed = copy.deepcopy(context)
        del changed['current_qualification']
        with self.assertRaisesRegex(gn.NeutralityError, 'current_qualification'):
            self.admit(changed)
        changed = copy.deepcopy(context)
        changed['qualifications']['19']['helper_sha256'] = '0' * 64
        with self.assertRaisesRegex(gn.NeutralityError, 'unbound historical'):
            self.admit(changed)

    def test_task19_binding_link_digest_and_path_are_required(self):
        context, _, b19 = self.finish_context()
        receipt_path = Path(b19['receipt_path'])
        original = json.loads(receipt_path.read_text())
        for field in ('binding_path', 'binding_sha256'):
            for value in (None, '0' * 64 if field.endswith('sha256') else context['bindings']['19']):
                reference = copy.deepcopy(b19['qualification_reference'])
                if value is None:
                    del reference[field]
                else:
                    reference[field] = value
                receipt = dict(original, qualification_reference=reference)
                receipt_path.write_bytes(gn._encoded(receipt))
                binding = dict(b19, qualification_reference=reference,
                               receipt_sha256=gn.sha256(receipt_path.read_bytes()))
                self.put('binding-19.json', binding)
                changed = copy.deepcopy(context)
                changed['qualifications']['19'] = reference
                with self.subTest(field=field, value=value), self.assertRaises(gn.NeutralityError):
                    self.admit(changed)

    def test_current_regeneration_runs_twice_even_when_receipt_exists(self):
        context, _, _ = self.finish_context()
        cargo = self.fake_bin / 'cargo'
        count = self.evidence_root / 'calls'
        cargo.write_text('#!/bin/sh\necho call >> "' + str(count) + '"\n')
        gn.verify_finish(self.admit(context))
        first = len(count.read_text().splitlines())
        gn.verify_finish(self.admit(context))
        self.assertEqual(len(count.read_text().splitlines()), first * 2)

    def test_second_current_regeneration_must_also_be_reproducible(self):
        context, _, _ = self.finish_context()
        marker = self.evidence_root / 'first-regen'
        script = self.repo / 'tools/regen-all.sh'
        original = script.read_text()
        # The fake cargo uses the regen shell's parent PID to distinguish
        # first and second invocations without changing qualified sources.
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('#!/bin/sh\nif [ -f "' + str(marker) + '" ]; then\n'
                         'old=$(cat "' + str(marker) + '")\n'
                         'if [ "$old" != "$PPID" ]; then printf "changed" > "'
                         + str(self.output) + '"; fi\n'
                         'else echo "$PPID" > "' + str(marker) + '"; fi\n')
        with self.assertRaisesRegex(gn.NeutralityError, 'not reproducible'):
            gn.verify_finish(self.admit(context))
        self.assertFalse(list(self.evidence_root.glob('finish/*/receipt.json')))
        self.assertEqual(script.read_text(), original)

    def test_finish_generator_failure_has_no_passing_receipt(self):
        context, _, _ = self.finish_context()
        (self.fake_bin / 'cargo').write_text('#!/bin/sh\nprintf "failed" >&2\nexit 9\n')
        with self.assertRaisesRegex(gn.NeutralityError, 'regeneration failed'):
            gn.verify_finish(self.admit(context))
        self.assertFalse(list(self.evidence_root.glob('finish/*/receipt.json')))

    def test_accepted_binding_and_finish_current_replay_preserve_pins(self):
        context, _, _ = self.finish_context()
        # A later authorized output change is not compared to either old baseline.
        self.output.write_text('later generated bytes\n')
        self.write('schemas/current.yaml', 'later schema input\n')
        context['finish_head'] = self.commit()
        cargo = self.fake_bin / 'cargo'
        cargo.write_text('#!/bin/sh\n' + self.generation_script().replace('generated bytes', 'later generated bytes'))
        self.write('bindings/python/README.md', 'dirty handwritten finish bytes\n')
        admitted = self.admit(context)
        pins = self.pins()
        receipt = gn.verify_finish(admitted)
        self.assertEqual(receipt['finish_head'], context['finish_head'])
        self.assertEqual(len(receipt['commands']), 2)
        self.assertNotEqual(receipt['before_manifest_sha256'], json.loads(
            (self.evidence_root / 'prelaunch.json').read_text())['before_manifest_sha256'])
        for path, data in pins.items():
            self.assertEqual((self.evidence_root / path).read_bytes(), data)
        after = self.pins()
        self.assertEqual(gn.verify_finish(self.admit(context)), receipt)
        self.assertEqual(after, self.pins())
        self.assertEqual(self.cli().returncode, 0)
        gn.verify_finish(self.admit(context, task='19'))
        result = self.cli('--exercise-nine')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.repo / 'bindings/python/README.md').read_text(), 'dirty handwritten finish bytes\n')


class QualificationSuiteCollectionTests(unittest.TestCase):
    def test_frozen_focused_names_resolve_and_full_collection_is_unique(self):
        module = sys.modules[__name__]
        loader = unittest.defaultTestLoader
        for name in _FROZEN_CONTRACT_FOCUSED_NAMES:
            with self.subTest(name=name):
                cases = list(_suite_cases(loader.loadTestsFromName(
                    'PrivateRecoveryTests.' + name, module)))
                self.assertEqual(len(cases), 1)
                self.assertIsInstance(cases[0], PrivateRecoveryTests)
                self.assertEqual(cases[0]._testMethodName, name)
        full = list(_suite_cases(loader.loadTestsFromModule(module)))
        names = [test.id() for test in full]
        self.assertEqual(len(names), len(set(names)))
        self.assertFalse(set(_FROZEN_CONTRACT_ALIASES).intersection(
            test._testMethodName for test in full))
        # The hook must preserve every real test, not merely produce a unique
        # subset. Build the unfiltered inventory without re-entering load_tests.
        raw = [test for value in vars(module).values()
               if isinstance(value, type) and issubclass(value, unittest.TestCase)
               for test in loader.loadTestsFromTestCase(value)]
        expected = [test.id() for test in raw
                    if test._testMethodName not in _FROZEN_CONTRACT_ALIASES]
        self.assertCountEqual(names, expected)

    def test_partial_alias_isolates_fixtures_and_reports_both_results(self):
        events = []

        def setup(test):
            events.append(('setup', test))
            test.addCleanup(events.append, ('cleanup', test))

        def failure(test):
            events.append(('failure', test))
            test.fail('first component failure')

        def error(test):
            events.append(('error', test))
            raise RuntimeError('second component error')

        with patch.object(PrivateRecoveryTests, 'setUp', setup), \
             patch.object(PrivateRecoveryTests, 'tearDown',
                          lambda test: events.append(('teardown', test))), \
             patch.object(PrivateRecoveryTests,
                          'test_partial_acquisition_interrupted_payload_finalize', failure), \
             patch.object(PrivateRecoveryTests,
                          'test_pin_order_refusal_before_generator_execution', error):
            with self.assertRaises(AssertionError) as raised:
                PrivateRecoveryTests('test_partial_acquisition_and_pin_order'
                                     ).test_partial_acquisition_and_pin_order()
        first, second = events[0][1], events[4][1]
        self.assertIsNot(first, second)
        self.assertEqual(events, [('setup', first), ('failure', first),
                                  ('teardown', first), ('cleanup', first),
                                  ('setup', second), ('error', second),
                                  ('teardown', second), ('cleanup', second)])
        self.assertIn('first component failure', str(raised.exception))
        self.assertIn('second component error', str(raised.exception))


class QualificationSuiteBytecodeTests(unittest.TestCase):
    def test_documented_invocation_writes_no_bytecode(self):
        # C7: the suite loads the helper by path; under ordinary Python
        # defaults that would leave tools/__pycache__ in the working-state
        # snapshot (ignored by Git, unbindable at finish). Run the documented
        # command in a synthetic copy without PYTHONDONTWRITEBYTECODE.
        with tempfile.TemporaryDirectory(prefix='neutrality-bytecode-') as name:
            tools = Path(name) / 'tools'
            tools.mkdir()
            shutil.copyfile(HELPER, tools / HELPER.name)
            shutil.copyfile(Path(__file__).resolve(), tools / Path(__file__).name)
            env = {k: v for k, v in os.environ.items()
                   if k not in ('PYTHONDONTWRITEBYTECODE', 'PYTHONPYCACHEPREFIX')}
            result = subprocess.run(['python3', 'tools/' + Path(__file__).name,
                                     'ScopeTests.test_source_derived_domains_and_hashes'],
                                    cwd=name, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual([p.relative_to(name).as_posix() for p in tools.rglob('__pycache__')], [])


class QualificationSuiteEnvironmentTests(unittest.TestCase):
    def test_documented_invocation_ignores_ambient_launch_overrides(self):
        # C5r2: the suite's verdict depends on the helper, not on the
        # operator's shell. Ordinary Rust settings the guard refuses are
        # exported to a child run of the documented command on a selection
        # that drives the real launch path; the refusal stays covered by its
        # dedicated test, which this selection also runs.
        with tempfile.TemporaryDirectory(prefix='neutrality-ambient-') as name:
            tools = Path(name) / 'tools'
            tools.mkdir()
            shutil.copyfile(HELPER, tools / HELPER.name)
            shutil.copyfile(Path(__file__).resolve(), tools / Path(__file__).name)
            env = dict(os.environ, PYTHONDONTWRITEBYTECODE='1', RUSTFLAGS='-D warnings',
                       CARGO_BUILD_TARGET='x86_64-unknown-linux-musl')
            result = subprocess.run(
                ['python3', 'tools/' + Path(__file__).name,
                 'ContextTaskTests.test_caller_bash_env_hook_cannot_precede_the_recipe',
                 'NineTests.test_caller_rustc_wrapper_refuses_before_any_mutation'],
                cwd=name, env=env, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('Ran 2 tests', result.stderr)


@unittest.skipUnless(os.environ.get('KCR_NEUTRALITY_SMOKE') == '1',
                     'optional real-domain smoke; set KCR_NEUTRALITY_SMOKE=1')
class RealDomainSmoke(unittest.TestCase):
    def test_real_domains_are_nonempty(self):
        scope = gn.derive_scope(HELPER.parent.parent)
        self.assertGreater(len(scope['output_rules']), 0)
        for rule in scope['output_rules']:
            print('DOMAIN ' + rule['root'])
        for source in scope['scope_sources']:
            if source['path'] == MAIN or source['path'].startswith('crates/openxcvr-codegen/src/emitters/'):
                print('AUDITED EMITTER ' + source['path'])


if __name__ == '__main__':
    unittest.main(verbosity=2)
