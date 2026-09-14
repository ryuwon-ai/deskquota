"""Accounting-ablation profiles, provenance checks, and paired summaries."""

import copy
import hashlib
import math
import re
import tomllib
from collections import Counter
from pathlib import Path


ACCOUNTING_ARMS = ("production_rr", "production_actual")
BASELINE_GATEWAY_ARMS = ("production_rr", "benchmark_fifo", "benchmark_rr")
_UNSET = object()


def _is_finite_number(value):
    return type(value) is int or (type(value) is float and math.isfinite(value))


def accounting_for_arm(arm):
    if arm == "production_actual":
        return "actual"
    if arm in BASELINE_GATEWAY_ARMS or arm == "benchmark_backfill":
        return "reserved"
    raise ValueError("this arm does not launch a gateway")


def paired_schedule(seeds, phase="quota"):
    seeds = list(seeds)
    if len(seeds) != len(set(seeds)):
        raise ValueError("duplicate planned seed")
    schedule = []
    for index, seed in enumerate(seeds):
        order = ACCOUNTING_ARMS if index % 2 == 0 else ACCOUNTING_ARMS[::-1]
        schedule.extend((phase, seed, arm) for arm in order)
    return schedule


def _render_gateway_config(accounting, listen_port, upstream_port, quota, cap, roots):
    text = (
        f'listen = "127.0.0.1:{listen_port}"\n'
        f"concurrency = {cap}\n"
        'cancel_policy = "close"\n'
        f'accounting = "{accounting}"\n'
        "retry_transient_429 = false\n"
        "[upstream]\n"
        f'api_base = "http://127.0.0.1:{upstream_port}/v1"\n'
        "[upstream.auth]\n"
        'mode = "none"\n'
    )
    for name in ("rpm", "tpm"):
        text += f"\n[quota.{name}]\n"
        text += f'kind = "{"known" if quota else "unlimited"}"\n'
        if quota:
            text += f'value = {quota[name]}\n'
    text += '\n[[models]]\nid = "synthetic"\nmax_output_tokens = 4096\n'
    for index in range(roots):
        text += (
            f'\n[[roots]]\nid = "r{index}"\n'
            'endpoints = ["chat/completions", "models"]\n'
            'models = ["synthetic"]\n'
        )
    return text


def _sanitized_snapshot(text):
    parsed = tomllib.loads(text)
    auth = parsed.get("upstream", {}).get("auth", {})
    if auth != {"mode": "none"}:
        raise ValueError("benchmark config must use auth none without credentials")
    return parsed


def build_gateway_config(
    *, arm, listen_port, upstream_port, quota, launched_binary, cap=2, roots=4
):
    accounting = accounting_for_arm(arm)
    text = _render_gateway_config(
        accounting, listen_port, upstream_port, quota, cap, roots
    )
    metadata = {
        "raw_toml_sha256": hashlib.sha256(text.encode()).hexdigest(),
        "snapshot": _sanitized_snapshot(text),
        "listen_port": listen_port,
        "upstream_port": upstream_port,
        "concurrency": cap,
        "roots": roots,
        "quota": quota,
        "cancel_policy": "close",
        "accounting": accounting,
        "retry_transient_429": False,
        "launched_binary": launched_binary,
    }
    return text, metadata


def validate_config_provenance(
    metadata, *, expected_arm, expected_quota=_UNSET, cap=2, roots=4
):
    expected_accounting = accounting_for_arm(expected_arm)
    if metadata.get("accounting") != expected_accounting:
        raise ValueError("declared accounting does not match arm")
    if expected_quota is not _UNSET and metadata.get("quota") != expected_quota:
        raise ValueError("declared quota does not match run")
    launched = metadata.get("launched_binary")
    if not isinstance(launched, dict) or launched.get("features") != []:
        raise ValueError("ordinary product binary identity required")
    ports = (metadata.get("listen_port"), metadata.get("upstream_port"))
    if not all(type(port) is int and 1 <= port <= 65535 for port in ports):
        raise ValueError("recorded loopback port is invalid")
    try:
        text = _render_gateway_config(
            expected_accounting,
            metadata["listen_port"],
            metadata["upstream_port"],
            metadata["quota"],
            cap,
            roots,
        )
    except (KeyError, TypeError) as error:
        raise ValueError("incomplete generated config provenance") from error
    expected_snapshot = _sanitized_snapshot(text)
    if metadata.get("raw_toml_sha256") != hashlib.sha256(text.encode()).hexdigest():
        raise ValueError("generated TOML hash mismatch")
    if metadata.get("snapshot") != expected_snapshot:
        raise ValueError("generated TOML snapshot mismatch")
    if metadata.get("concurrency") != cap or metadata.get("roots") != roots:
        raise ValueError("config concurrency/root count mismatch")
    if (
        metadata.get("cancel_policy") != "close"
        or metadata.get("retry_transient_429") is not False
    ):
        raise ValueError("config cancellation/retry policy mismatch")
    return expected_snapshot


def validate_written_config(
    path, metadata, *, expected_arm, expected_quota=_UNSET, cap=2, roots=4
):
    """Return provenance derived from the exact TOML bytes that will be launched."""
    raw = Path(path).read_bytes()
    try:
        snapshot = _sanitized_snapshot(raw.decode("utf-8"))
    except (UnicodeDecodeError, tomllib.TOMLDecodeError) as error:
        raise ValueError("written TOML is invalid") from error
    raw_sha256 = hashlib.sha256(raw).hexdigest()
    accounting = snapshot.get("accounting")
    if (
        metadata.get("raw_toml_sha256") != raw_sha256
        or metadata.get("snapshot") != snapshot
        or metadata.get("accounting") != accounting
    ):
        raise ValueError("written TOML does not match generated metadata")
    actual = copy.deepcopy(metadata)
    actual.update(
        raw_toml_sha256=raw_sha256,
        snapshot=snapshot,
        accounting=accounting,
    )
    validate_config_provenance(
        actual,
        expected_arm=expected_arm,
        expected_quota=expected_quota,
        cap=cap,
        roots=roots,
    )
    return actual


def validate_runtime_config(metadata, status):
    if status.get("identity", {}).get("fingerprint") != metadata.get(
        "raw_toml_sha256"
    ):
        raise ValueError("runtime config fingerprint mismatch")
    if status.get("admission", {}).get("accounting") != metadata.get("accounting"):
        raise ValueError("runtime accounting mismatch")
    return True


def normalize_config_snapshot(snapshot, generated_paths=()):
    normalized = copy.deepcopy(snapshot)
    replacements = [(str(path), "<generated-path>") for path in generated_paths]
    loopback_port = re.compile(r"(?<=127\.0\.0\.1:)\d+")

    def visit(value):
        if isinstance(value, dict):
            return {key: visit(child) for key, child in value.items()}
        if isinstance(value, list):
            return [visit(child) for child in value]
        if isinstance(value, str):
            result = loopback_port.sub("<loopback-port>", value)
            for source, target in replacements:
                result = result.replace(source, target)
            return result
        return value

    return visit(normalized)


def differing_paths(left, right, prefix=""):
    if type(left) is not type(right):
        return [prefix or "<root>"]
    if isinstance(left, dict):
        paths = []
        for key in sorted(set(left) | set(right)):
            child = f"{prefix}.{key}" if prefix else key
            if key not in left or key not in right:
                paths.append(child)
            else:
                paths.extend(differing_paths(left[key], right[key], child))
        return paths
    if isinstance(left, list):
        if len(left) != len(right):
            return [prefix]
        paths = []
        for index, (a, b) in enumerate(zip(left, right)):
            paths.extend(differing_paths(a, b, f"{prefix}[{index}]"))
        return paths
    return [] if left == right else [prefix]


def validate_accounting_run(
    run,
    *,
    expected_arm=None,
    expected_seed=None,
    expected_phase=None,
    expected_rows=None,
    expected_duration_s=None,
    expected_windows=None,
    expected_quota=_UNSET,
    expected_binary=None,
):
    arm = expected_arm if expected_arm is not None else run.get("arm")
    if arm not in ACCOUNTING_ARMS or run.get("arm") != arm:
        raise ValueError("accounting run arm mismatch")
    if expected_seed is not None and run.get("seed") != expected_seed:
        raise ValueError("accounting run seed mismatch")
    if expected_phase is not None and run.get("phase") != expected_phase:
        raise ValueError("accounting run phase mismatch")
    if (
        expected_duration_s is not None
        and run.get("measurement_duration_s") != expected_duration_s
    ):
        raise ValueError("accounting measurement duration mismatch")
    if expected_windows is not None and run.get("quota_windows") != expected_windows:
        raise ValueError("accounting quota window count mismatch")
    run_quota = run.get("mock_quota", _UNSET)
    if run_quota is _UNSET:
        raise ValueError("accounting mock quota missing")
    if expected_quota is not _UNSET and run_quota != expected_quota:
        raise ValueError("accounting mock quota mismatch")
    if expected_rows is not None and run.get("submitted") != expected_rows:
        raise ValueError("accounting submitted schedule mismatch")
    validate_config_provenance(
        run.get("config", {}), expected_arm=arm, expected_quota=run_quota
    )
    if (
        expected_binary is not None
        and run["config"].get("launched_binary") != expected_binary
    ):
        raise ValueError("launched binary identity mismatch")
    for field in ("initial_status", "final_status"):
        status = run.get(field)
        if not isinstance(status, dict):
            raise ValueError(f"{field} worker status missing")
        try:
            validate_runtime_config(run["config"], status)
        except ValueError as error:
            raise ValueError(f"{field} {error}") from error

    submitted_rows = run.get("submitted", [])
    outcomes = run.get("outcomes", [])
    attempts = run.get("attempts", [])
    submitted = {row["id"]: row for row in submitted_rows}
    if len(submitted) != len(submitted_rows):
        raise ValueError("duplicate submitted ID")
    terminal_ids = [outcome.get("id") for outcome in outcomes]
    if (
        len(set(terminal_ids)) != len(terminal_ids)
        or set(terminal_ids) != set(submitted)
    ):
        raise ValueError("terminal denominator mismatch")
    allowed_outcomes = {"completed", "rejected", "error", "timeout", "cancelled"}
    if any(outcome.get("outcome") not in allowed_outcomes for outcome in outcomes):
        raise ValueError("nonterminal outcome")
    start = run.get("start_monotonic_s")
    duration = run.get("measurement_duration_s")
    if not _is_finite_number(start) or not _is_finite_number(duration):
        raise ValueError("measurement cutoff fields must be finite numbers")
    try:
        cutoff = start + duration
    except OverflowError as error:
        raise ValueError("measurement cutoff must be finite") from error
    if not _is_finite_number(cutoff):
        raise ValueError("measurement cutoff must be finite")
    attempts_by_ingress = {}
    for attempt in attempts:
        attempts_by_ingress.setdefault(attempt.get("ingress_id"), []).append(attempt)
    for outcome in outcomes:
        ended = outcome.get("ended_s")
        if not _is_finite_number(ended):
            raise ValueError("terminal ended_s must be finite")
        recomputed = ended <= cutoff
        if outcome.get("within_measurement") is not recomputed:
            raise ValueError("within_measurement flag disagrees with cutoff")
        row = submitted[outcome["id"]]
        if outcome["outcome"] != "completed":
            continue
        if outcome.get("payload_valid") is not True:
            raise ValueError("completion payload validation missing")
        if row.get("metadata", False):
            if outcome.get("usage") != {"status": "not_applicable"}:
                raise ValueError("metadata usage must be not_applicable")
            continue
        usage = outcome.get("usage")
        if not isinstance(usage, dict) or usage.get("status") != "observed":
            raise ValueError("completed generation usage not observed")
        usage_values = (
            usage.get("prompt_tokens"),
            usage.get("completion_tokens"),
        )
        if any(
            type(value) is not int or not 0 <= value <= 2**64 - 1
            for value in usage_values
        ):
            raise ValueError("completed generation usage must contain u64 integers")
        completed_attempts = [
            attempt
            for attempt in attempts_by_ingress.get(outcome["id"], [])
            if attempt.get("outcome") == "completed"
        ]
        if len(completed_attempts) != 1:
            raise ValueError("completed generation attempt linkage mismatch")
        attempt = completed_attempts[0]
        expected_input = math.ceil(attempt["body_bytes"] * row["actual_ratio"])
        expected_output = max(
            1, math.ceil(row["output_reservation"] * row["actual_ratio"])
        )
        if (
            usage.get("prompt_tokens") != expected_input
            or usage.get("completion_tokens") != expected_output
            or expected_input + expected_output != attempt.get("actual_cost_fixture_units")
        ):
            raise ValueError("completed generation usage/attempt mismatch")
    return True


def validate_accounting_pair(reserved, actual):
    validate_accounting_run(reserved)
    validate_accounting_run(actual)
    if (
        reserved.get("seed") != actual.get("seed")
        or reserved.get("phase") != actual.get("phase")
    ):
        raise ValueError("paired run seed/phase mismatch")
    if reserved.get("submitted") != actual.get("submitted"):
        raise ValueError("paired submitted schedules differ")
    if (
        reserved.get("measurement_duration_s")
        != actual.get("measurement_duration_s")
    ):
        raise ValueError("paired measurement cutoffs differ")
    if reserved.get("config", {}).get("launched_binary") != actual.get(
        "config", {}
    ).get("launched_binary"):
        raise ValueError("paired product binary identities differ")
    left = normalize_config_snapshot(reserved["config"]["snapshot"])
    right = normalize_config_snapshot(actual["config"]["snapshot"])
    if differing_paths(left, right) != ["accounting"]:
        raise ValueError("paired normalized configs differ outside accounting")
    return True


def _distribution(values):
    values = sorted(values)
    if not values:
        return {"n": 0, "p50": None, "p95": None, "max": None}

    def percentile(fraction):
        return values[min(len(values) - 1, math.ceil(len(values) * fraction) - 1)]

    return {
        "n": len(values),
        "p50": percentile(0.5),
        "p95": percentile(0.95),
        "max": values[-1],
    }


def _arm_summary(run):
    submitted = {row["id"]: row for row in run["submitted"]}
    within = [row for row in run["outcomes"] if row["within_measurement"]]
    attempts_per_ingress = Counter(row["ingress_id"] for row in run["attempts"])
    groups = {}
    for field in ("root", "length"):
        values = sorted({row[field] for row in submitted.values()}, key=str)
        groups[field] = {
            str(value): {
                "submitted": sum(row[field] == value for row in submitted.values()),
                "within_measurement_completed": sum(
                    row["outcome"] == "completed" and submitted[row["id"]][field] == value
                    for row in within
                ),
            }
            for value in values
        }
    scheduled = {
        row["id"]
        for row in submitted.values()
        if row.get("cancel_after_ms") is not None
    }
    return {
        "submitted": len(submitted),
        "within_measurement_terminal": dict(Counter(row["outcome"] for row in within)),
        "within_measurement_completed": sum(
            row["outcome"] == "completed" for row in within
        ),
        "post_measurement_drain_completed": sum(
            row["outcome"] == "completed" and not row["within_measurement"]
            for row in run["outcomes"]
        ),
        "all_terminal": dict(Counter(row["outcome"] for row in run["outcomes"])),
        "root_results": groups["root"],
        "length_results": groups["length"],
        "mock_429_attempts": sum(
            row["outcome"] == "rejected" for row in run["attempts"]
        ),
        "upstream_attempts": len(run["attempts"]),
        "additional_attempts_beyond_first": sum(
            max(0, count - 1) for count in attempts_per_ingress.values()
        ),
        "scheduled_cancellations": len(scheduled),
        "scheduled_cancellation_outcomes": dict(
            Counter(row["outcome"] for row in run["outcomes"] if row["id"] in scheduled)
        ),
        "completion_ms_success_only": _distribution(
            [row["elapsed_ms"] for row in run["outcomes"] if row["outcome"] == "completed"]
        ),
    }


def paired_summary(runs, planned_seeds, phase):
    planned_seeds = list(planned_seeds)
    if len(planned_seeds) != len(set(planned_seeds)):
        raise ValueError("duplicate planned seed")
    if phase == "smoke":
        expected_duration, expected_windows = 3, 0
    elif phase == "quota":
        expected_duration, expected_windows = 300, 5
    else:
        raise ValueError("paired accounting phase must be smoke or quota")
    keys = [(run.get("phase"), run.get("seed"), run.get("arm")) for run in runs]
    if len(keys) != len(set(keys)):
        raise ValueError("duplicate accounting run in paired summary")
    by_key = {
        (run["seed"], run["arm"]): run
        for run in runs
        if run.get("phase") == phase
    }
    pairs = []
    for seed in planned_seeds:
        reserved = by_key.get((seed, "production_rr"))
        actual = by_key.get((seed, "production_actual"))
        if reserved is None or actual is None:
            continue
        for run in (reserved, actual):
            validate_accounting_run(
                run,
                expected_phase=phase,
                expected_duration_s=expected_duration,
                expected_windows=expected_windows,
            )
        validate_accounting_pair(reserved, actual)
        arm_results = {
            "production_rr": _arm_summary(reserved),
            "production_actual": _arm_summary(actual),
        }
        pairs.append(
            {
                "seed": seed,
                "arms": arm_results,
                "actual_minus_reserved_within_measurement_completed": (
                    arm_results["production_actual"]["within_measurement_completed"]
                    - arm_results["production_rr"]["within_measurement_completed"]
                ),
            }
        )
    if phase == "smoke":
        scope = "smoke_validation_only_no_efficiency_conclusion"
    elif len(pairs) == 1:
        scope = "one_pair_checkpoint_only_no_multi_seed_conclusion"
    else:
        scope = "paired_descriptive_results_no_broad_performance_claim"
    return {
        "phase": phase,
        "planned_pairs": len(planned_seeds),
        "completed_pairs": len(pairs),
        "pairs": pairs,
        "interpretation_scope": scope,
        "client_usage_observation_limit": "observed response usage is not proof of gateway internal refund",
    }
