"""Compiler/runtime provenance integrity fixtures, independent of a built compiler."""
import copy
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[2]/'tools'))
import golden_runtime_provenance as provenance
import golden_cases as harness


class TemporalProvenanceTests(unittest.TestCase):
    def test_enum_bytes_and_host_reports_are_separate_and_bound(self):
        zone = provenance.ROOT/'jadpo/data/iana-zones-2026c.txt'
        native = {'reportedIcu': 'fixture-icu', 'engine': 'fixture-engine',
                  'reportedTimezoneDataVersion': 'unreported'}
        generated = {'tzdb': 'IANA 2026c', 'tzdbScope': 'compiler_zone_enumeration_only',
                     'zoneEnumVersion': 'IANA 2026c', 'zoneEnumSource': str(zone.relative_to(provenance.ROOT)),
                     'zoneEnumByteDigest': 'sha256:'+harness.digest(zone),
                     'zoneEnumCount': len(set(zone.read_text().splitlines())),
                     'runtimeRuleProvider': 'ECMAScript Intl.DateTimeFormat',
                     'runtimeRuleDataVersion': 'unreported', 'runtimeRuleDataVersionSource': 'process.versions.tz',
                     'runtimeRuleDataByteDigest': None, 'runtimeIcu': 'fixture-icu',
                     'runtimeEngine': 'fixture-engine', 'timezoneRules': 'runtime ICU; version recorded for release pinning'}
        provenance.validate_temporal_metadata(generated, native, zone)
        for key in generated:
            with self.subTest(key=key):
                mutated = copy.deepcopy(generated)
                mutated[key] = 'unbound-forgery'
                with self.assertRaises(ValueError):
                    provenance.validate_temporal_metadata(mutated, native, zone)
        # A reported runtime version is retained as reported; it need not equal the enum.
        native['reportedTimezoneDataVersion'] = 'fixture-rule-version'
        generated['runtimeRuleDataVersion'] = 'fixture-rule-version'
        provenance.validate_temporal_metadata(generated, native, zone)
        generated['runtimeRuleDataVersion'] = '2026c'
        with self.assertRaises(ValueError):
            provenance.validate_temporal_metadata(generated, native, zone)


if __name__ == '__main__': unittest.main()
