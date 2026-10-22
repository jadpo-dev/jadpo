"""Benchmark verification must fail closed before timings can be compared."""
import unittest
from contracts import validate_measurement, validate_list, validate_sample, validate_write_count, validate_failure

class Contracts(unittest.TestCase):
    def test_failure_contract_requires_the_decided_status_and_code(self):
        body = {"error": {"code": "invalid_input", "message": "Request input is invalid.", "request_id": "req_test"}}
        validate_failure(422, body, 422, "invalid_input")
        for status, code in [(400, "invalid_input"), (422, "wrong")]:
            with self.assertRaises(ValueError):
                validate_failure(status, body, 422, code)
        for invalid_body in (None, [], "invalid"):
            with self.assertRaises(ValueError):
                validate_failure(422, invalid_body, 422, "invalid_input")
        with self.assertRaises(ValueError):
            validate_failure(422, {"error": {**body["error"], "credential": "leaked"}}, 422, "invalid_input")

    def test_corrupt_200_body_is_rejected(self):
        with self.assertRaises(ValueError):
            validate_sample({"id": "sample", "title": "small", "payload": "wrong"}, "sample", "small", "x" * 100)

    def test_correct_large_body_is_checked_in_full(self):
        payload = "x" * 65536
        validate_sample({"id": "sample", "title": "large", "payload": payload}, "sample", "large", payload)
        with self.assertRaises(ValueError):
            validate_sample({"id": "sample", "title": "large", "payload": payload[:-1]}, "sample", "large", payload)

    def test_owner_leak_and_unstable_order_are_rejected(self):
        expected = [{"id": "1", "title": "a"}, {"id": "2", "title": "b"}]
        validate_list(expected, expected)
        for wrong in [expected[::-1], expected + [{"id": "other", "title": "z"}], {"items": expected}, []]:
            with self.assertRaises(ValueError):
                validate_list(wrong, expected)

    def test_no_silent_error_or_empty_measurement(self):
        good = {"requests": 1, "errors": 0, "unexpected": 0, "invalid_bodies": 0, "p95_us": 5, "rps": 1, "warmup_errors": 0, "warmup_invalid_bodies": 0, "all_unexpected": 0}
        validate_measurement(good)
        for field in ["errors", "unexpected", "invalid_bodies", "warmup_errors", "warmup_invalid_bodies", "all_unexpected"]:
            with self.assertRaises(ValueError):
                validate_measurement({**good, field: 1})
        for field in ["requests", "p95_us", "rps"]:
            with self.assertRaises(ValueError):
                validate_measurement({**good, field: 0})

    def test_nonfinite_or_negative_metrics_are_rejected(self):
        good = {"requests": 1, "errors": 0, "unexpected": 0, "invalid_bodies": 0, "p95_us": 5, "rps": 1, "warmup_errors": 0, "warmup_invalid_bodies": 0, "all_unexpected": 0}
        for invalid in [float('nan'), float('inf'), -1]:
            with self.assertRaises(ValueError):
                validate_measurement({**good, "elapsed_s": invalid})

    def test_writes_include_warmup_and_boundary_requests(self):
        validate_write_count(before=50, after=57, successes=7)
        with self.assertRaises(ValueError):
            validate_write_count(before=50, after=56, successes=7)

if __name__ == "__main__":
    unittest.main()
