"""Synthetic structural tests; full fixture-contract tests need maintainer inputs."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest
import yaml

_SPEC = importlib.util.spec_from_file_location('parity_checker', Path(__file__).with_name('check-parity-fixtures.py'))
p = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(p)


def synthetic_shape():
    shape = {key: ['example'] for key in p.SHAPE_KEYS}
    shape.update(schema='openxcvr.parity.shape.v1', kind='parity_fixture_shape',
                 metadata_files=['_shape.yaml', 'donor_inventory.yaml', 'differences.yaml'],
                 case_fields={'optional': []}, citation={'standard': [], 'no_standard': []},
                 donor={'dispositions': {'executable': {}, 'source_only': {}, 'none': {}},
                        'source_fields': ['example'], 'passes': ['example'], 'optional': ['example'],
                        'citable_rows': {'example': 'example'}},
                 differences={'required': ['id'], 'required_ids': ['example'], 'comparisons': ['example']},
                 inventory={'matrix_rows': {'example': 'Example'}, 'rows': {'example': {
                     'matrix_row': 'example', 'disposition': 'source_only_comparison',
                     'routine_owner': 'task1', 'adapter_id': 'none'}}},
                 adapters={a: {'legacy': a in p.LEGACY_KMOD_FUNCTIONS, 'kinds': list(p.KINDS),
                               'families': ['example'], 'donor_paths': [] if a == 'none' else ['example.c']}
                           for a in p.provenance.VALID_ADAPTERS},
                 kinds={kind: {'file': kind + '.yaml', 'required_inputs': [], 'required_facets': [],
                               'expected_types': {'example': ['example']},
                               'categories': {'example': {'required': True, 'requires': {'id': 'present'}}}}
                        for kind in p.KINDS})
    return shape


class StructuralTests(unittest.TestCase):
    def test_synthetic_shape(self):
        self.assertEqual(p.validate_shape(synthetic_shape())['kind'], 'parity_fixture_shape')

    def test_shape_unknown_key(self):
        shape = synthetic_shape()
        shape['typo'] = True
        with self.assertRaisesRegex(p.ParityFixtureError, 'unknown keys'):
            p.validate_shape(shape)

    def test_shape_missing_adapter(self):
        shape = synthetic_shape()
        del shape['adapters']['none']
        with self.assertRaisesRegex(p.ParityFixtureError, 'adapter registry'):
            p.validate_shape(shape)

    def test_no_required_category_is_refused(self):
        shape = synthetic_shape()
        shape['kinds']['thermal']['categories']['example']['required'] = False
        with self.assertRaisesRegex(p.ParityFixtureError, 'no required category'):
            p.validate_shape(shape)

    def test_unknown_predicate_is_refused(self):
        with self.assertRaisesRegex(p.ParityFixtureError, 'unknown predicate'):
            p.validate_predicate({'typo': 1}, 'example')

    def test_typed_predicates(self):
        obj = {'items': [{'bit': 1}, {'bit': 2}]}
        self.assertTrue(p.holds(obj, 'items.*.bit', {'contains_all': [1, 2]}))
        self.assertFalse(p.holds(obj, 'items.0.bit', {'equals': True}))
        self.assertTrue(p.holds(obj, 'missing', 'absent'))

    def test_duplicate_yaml_key_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'example.yaml'
            path.write_text('key: 1\nkey: 2\n')
            with self.assertRaisesRegex(p.ParityFixtureError, 'duplicate key'):
                p.load_yaml(path)

    def test_unknown_directory_input_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            (directory / 'surprise.yaml').write_text('example: true\n')
            with self.assertRaisesRegex(p.ParityFixtureError, 'unrecognized YAML'):
                p.scan_directory(directory, synthetic_shape())

    def test_closed_world_inventory(self):
        shape = synthetic_shape()
        row = dict(shape['inventory']['rows']['example'], id='example', matrix_title='Example',
                   source_path='example.c')
        self.assertEqual(set(p.validate_inventory([row], shape)), {'example'})
        with self.assertRaisesRegex(p.ParityFixtureError, 'pinned donor rows missing'):
            p.validate_inventory([], shape)
        wrong = copy.deepcopy(row)
        wrong['routine_owner'] = 'task2'
        with self.assertRaisesRegex(p.ParityFixtureError, 'differs from the pinned'):
            p.validate_inventory([wrong], shape)

    def test_missing_contract_fails(self):
        with tempfile.TemporaryDirectory() as tmp:
            with self.assertRaisesRegex(p.ParityFixtureError, 'file not found'):
                p.check(Path(tmp))


if __name__ == '__main__':
    unittest.main()
