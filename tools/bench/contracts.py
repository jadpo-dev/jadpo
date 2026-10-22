"""Shared, timing-independent assertions for the focused benchmark subset."""
import math


def validate_sample(actual, identity, title, payload):
    if actual != {"id": identity, "title": title, "payload": payload}:
        raise ValueError("Sample response differs from the seeded row")


def validate_list(actual, expected):
    if actual != expected:
        raise ValueError("Todo list differs from the owner's ordered seed")


def validate_measurement(row):
    for field in ("errors", "unexpected", "invalid_bodies", "warmup_errors", "warmup_invalid_bodies", "all_unexpected"):
        if row.get(field) != 0:
            raise ValueError(f"Measurement has {field}: {row.get(field)}")
    for field, value in row.items():
        if isinstance(value, (int, float)) and (not math.isfinite(value) or value < 0):
            raise ValueError(f"Measurement has an invalid {field}")
    for field in ("requests", "p95_us", "rps"):
        value = row.get(field, 0)
        if not isinstance(value, (int, float)) or not math.isfinite(value) or value <= 0:
            raise ValueError(f"Measurement has no valid {field}")


def validate_write_count(*, before, after, successes):
    if after - before != successes:
        raise ValueError("Persisted writes differ from successful requests, including warmup and drain")


def validate_failure(status, body, expected_status, code):
    error = body.get("error") if isinstance(body, dict) else None
    required = {"code", "message", "request_id"}
    if (status != expected_status or not isinstance(body, dict) or set(body) != {"error"}
            or not isinstance(error, dict) or not required <= error.keys()
            or not error.keys() <= required | {"details"}
            or error["code"] != code
            or not all(isinstance(error[key], str) and error[key] for key in required)):
        raise ValueError(f"Expected a safe {expected_status} {code} failure")
