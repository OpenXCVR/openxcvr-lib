"""Synthetic ledger tests. Real ledger/history assertions run with maintainer inputs."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

_NAME = Path(__file__).stem.removeprefix('test_').replace('_', '-') + '.py'
_SPEC = importlib.util.spec_from_file_location('ledger_checker', Path(__file__).with_name(_NAME))
p = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(p)


class LedgerTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.facts = Path(self.tmp.name) / 'authorship.json'
        self.data = {
            'ATTRIBUTED_IDENTITIES': {'Example <author@example.invalid>': 'Synthetic grant'},
            'RETAINED_OBSERVED_AUTHORS': {'core/example.c': ['Example <author@example.invalid>']},
            'RETAINED_OBSERVED_COMMITTERS': {'core/example.c': ['Example <author@example.invalid>']},
            'KMOD_DONOR_COMMIT': '1234567', 'KMOD_DONOR_COMMIT_SHA': '1234567' + '0' * 33,
            'KMOD_SPLIT_ROOT': 'abcdef0',
        }
        self.facts.write_text(json.dumps(self.data))
        p.load_authorship(self.facts)
        self.rows = [{'id': 'example', 'matrix_row': 'example', 'source_path': 'core/example.c',
                      'source_commit': '1234567', 'source_symbol': 'example', 'source_range': '1-2',
                      'disposition': 'source_only_comparison', 'routine_owner': 'task1',
                      'required_fixture_ids': [], 'adapter_id': 'none',
                      'source_only_gap': 'synthetic comparison only'}]
        section = {key: 'synthetic' for key in p.REQUIRED_SECTION_FIELDS}
        section.update(kmod_path='core/example.c', donor_commit='1234567', verdict='clear',
                       authors=['Example <author@example.invalid>'],
                       committers=['Example <author@example.invalid>'],
                       pre_split_history='checked: synthetic history', lib_commits='pending')
        self.sections = {'core/example.c': section}

    def validate(self):
        return p.validate_provenance(self.sections, self.rows)

    def test_valid_clear(self):
        self.assertEqual(self.validate()['cleared_count'], 1)

    def test_omitted_donor(self):
        self.sections.clear()
        with self.assertRaisesRegex(p.ProvenanceValidationError, 'omitted donor'):
            self.validate()

    def test_wrong_identity(self):
        self.sections['core/example.c']['authors'] = ['Other <other@example.invalid>']
        with self.assertRaisesRegex(p.ProvenanceValidationError, 'unattributed-plus-clear'):
            self.validate()

    def test_committer_history_is_closed_world(self):
        self.data['RETAINED_OBSERVED_COMMITTERS']['core/example.c'].append('Merge <merge@example.invalid>')
        self.facts.write_text(json.dumps(self.data))
        p.load_authorship(self.facts)
        with self.assertRaisesRegex(p.ProvenanceValidationError, 'omitted section committer'):
            self.validate()

    def test_clear_requires_positive_history(self):
        self.sections['core/example.c']['pre_split_history'] = 'unknown'
        with self.assertRaisesRegex(p.ProvenanceValidationError, 'not affirmatively checked'):
            self.validate()

    def test_stop_cannot_land(self):
        self.sections['core/example.c'].update(verdict='stop', lib_commits='1234567')
        with self.assertRaisesRegex(p.ProvenanceValidationError, 'stopped file cannot have landed'):
            self.validate()

    def test_missing_field(self):
        del self.sections['core/example.c']['grant']
        with self.assertRaisesRegex(p.ProvenanceValidationError, "missing required field 'grant'"):
            self.validate()

    def test_missing_and_malformed_facts_fail_closed(self):
        for text in ('{}', 'invalid json'):
            self.facts.write_text(text)
            with self.assertRaisesRegex(p.ProvenanceValidationError, '--authorship'):
                p.load_authorship(self.facts)
        self.facts.unlink()
        with self.assertRaisesRegex(p.ProvenanceValidationError, '--authorship'):
            p.load_authorship(self.facts)

    def test_partition_conflict(self):
        other = copy.deepcopy(self.rows[0])
        other.update(id='other', routine_owner='task2')
        with self.assertRaisesRegex(p.ProvenanceValidationError, 'overlap'):
            p.check_partition(self.rows + [other])

    def test_inventory_pin(self):
        import yaml
        self.rows[0]['source_commit'] = '7654321'
        with self.assertRaisesRegex(p.ProvenanceValidationError, 'source_commit'):
            p.parse_inventory(yaml.safe_dump(self.rows))


if __name__ == '__main__':
    unittest.main()
