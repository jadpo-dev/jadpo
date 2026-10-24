"""Independent canonical and integrity controls for the diagnostic Wasm screen."""
import copy
import json
import tempfile
import unittest
from pathlib import Path

from string_screen import canonical, case_specs, extract_writer, validate_build


class StringScreenControls(unittest.TestCase):
    def test_canonical_style_is_independent_of_short_backspace_formfeed(self):
        fixtures = [
            ('', b'""'),
            ('\b\f\n\r\t\0\x1f', b'"\\u0008\\u000c\\n\\r\\t\\u0000\\u001f"'),
            ('\\b\\f"\\', b'"\\\\b\\\\f\\"\\\\"'),
            ('e\u0301🙂\u2028\u2029\x7f', '"e\u0301🙂\u2028\u2029\x7f"'.encode()),
        ]
        for text, expected in fixtures:
            self.assertEqual(canonical(text), expected)
            self.assertEqual(json.loads(expected), text)

    def test_cases_include_exact_mixed_fixture_and_separate_ordinary_unicode(self):
        from request_profile import payloads
        cases = case_specs()
        self.assertEqual(len(cases), 6)
        by = {c['name']: c for c in cases}
        mixed = by['mixed-unicode']
        text = mixed['fragment']*mixed['repeats']+mixed['tail']
        self.assertEqual(text, payloads()['unicode-large'])
        self.assertEqual((len(text), len(text.encode())), (65536, 109228))
        ordinary = by['ordinary-unicode']
        plain = ordinary['fragment']*ordinary['repeats']+ordinary['tail']
        self.assertEqual(len(plain), 65536)
        self.assertTrue(all(c not in plain for c in ('"', '\\', '\n', '\t')))

    def test_extractor_rejects_missing_or_unbounded_writer(self):
        valid = 'earlier\npub fn write_string(text: &str, out: &mut String) {\n}\n\n#[cfg(test)]\nmod tests {}'
        self.assertEqual(extract_writer(valid), 'pub fn write_string(text: &str, out: &mut String) {\n}')
        for bad in ('no writer', valid.replace('#[cfg(test)]', 'other'), valid.replace('\n}', '\nother')):
            with self.assertRaises(ValueError):
                extract_writer(bad)

    def test_build_rejects_changed_source_artifact_cases_and_qualification(self):
        with tempfile.TemporaryDirectory() as raw:
            wasm = Path(raw)/'writers.wasm'
            wasm.write_bytes(b'\0asm\1\0\0\0')
            import hashlib
            from string_screen import case_metadata, VARIANTS, verification_count
            stamp = {'schemaVersion': 2, 'kind': 'string-screen', 'complete': True,
                     'qualification': 'diagnostic Node-hosted Wasm; not HTTP or workerd',
                     'source': {'head': 'frozen'}, 'opt_level': 's',
                     'artifacts': {str(wasm): hashlib.sha256(wasm.read_bytes()).hexdigest()},
                     'cases': case_metadata(), 'variants': list(VARIANTS),
                     'expected_verifications': verification_count(),
                     'codegen': {'indirect_call': True, 'black_box_barrier': True},
                     'canonical': {'passed': True, 'negative_controls': 2,
                                   'byte_checks': verification_count()-2}}
            validate_build(stamp, stamp['source'])
            for fault in (
                lambda s: s.update(schemaVersion=1),
                lambda s: s.update(complete=False),
                lambda s: s.update(opt_level='3'),
                lambda s: s['cases'].pop(),
                lambda s: s['canonical'].update(negative_controls=0),
                lambda s: s.update(qualification='complete HTTP contract'),
                lambda s: s['variants'].pop(),
                lambda s: s.update(expected_verifications=1),
                lambda s: s['canonical'].update(byte_checks=0),
                lambda s: s['codegen'].update(indirect_call=False),
                lambda s: s['codegen'].update(black_box_barrier=False),
            ):
                bad = copy.deepcopy(stamp); fault(bad)
                with self.subTest(fault=fault), self.assertRaises(ValueError):
                    validate_build(bad, stamp['source'])
            with self.assertRaises(ValueError):
                validate_build(stamp, {'head': 'other'})
            wasm.write_bytes(b'changed')
            with self.assertRaises(ValueError):
                validate_build(stamp, stamp['source'])


if __name__ == '__main__':
    unittest.main()
