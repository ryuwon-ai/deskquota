import argparse
import asyncio
import copy
import hashlib
import json
import math
import os
import sys
import tempfile
import unittest
from unittest import mock
from pathlib import Path


PRODUCT = Path(__file__).resolve().parents[1]
SCRIPTS = PRODUCT / "scripts"
sys.path.insert(0, str(SCRIPTS))

import benchmark
from benchmark_http import Client, encode, request_once


def generation_row(request_id="g", *, ratio=0.5):
    return {
        "id": request_id,
        "root": 0,
        "input_bytes": 10,
        "output_reservation": 8,
        "actual_ratio": ratio,
        "service_ms": 0,
        "timeout_s": 2,
        "offset_s": 0,
        "length": "short",
        "cost_case": "overestimated",
        "workload": "fixture",
        "window": 0,
        "workflow_id": "fixture-pair0",
    }


def worker_status(config, *, requests=3, upstream_attempts=0):
    return {
        "status": "ok",
        "state": "running",
        "requests": requests,
        "upstream_attempts": upstream_attempts,
        "identity": {
            "address": f'127.0.0.1:{config["listen_port"]}',
            "fingerprint": config["raw_toml_sha256"],
            "nonce": "b" * 64,
            "path_hash": "c" * 64,
            "pid": 1234,
            "started_unix_ms": 1,
        },
        "admission": {
            "accounting": config["accounting"],
            "active": 0,
            "queue_length": 0,
            "retained": 0,
            "roots": [
                {"id": f"r{index}", "queue_length": 0} for index in range(4)
            ],
        },
    }


async def one_response_server(frames):
    async def handle(reader, writer):
        await reader.readuntil(b"\r\n\r\n")
        body = b"".join(b"data: " + frame + b"\n\n" for frame in frames)
        writer.write(
            b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: "
            + str(len(body)).encode()
            + b"\r\n\r\n"
            + body
        )
        await writer.drain()
        writer.close()
        await writer.wait_closed()

    return await asyncio.start_server(handle, "127.0.0.1", 0)


class AccountingRulesTests(unittest.TestCase):
    def test_arm_profiles_and_alternating_pair_schedule(self):
        from benchmark_accounting import accounting_for_arm, paired_schedule

        self.assertEqual(accounting_for_arm("production_rr"), "reserved")
        self.assertEqual(accounting_for_arm("production_actual"), "actual")
        self.assertEqual(accounting_for_arm("benchmark_fifo"), "reserved")
        self.assertEqual(accounting_for_arm("benchmark_rr"), "reserved")
        self.assertEqual(accounting_for_arm("benchmark_backfill"), "reserved")
        self.assertNotIn("benchmark_backfill", benchmark.ARMS)
        with self.assertRaises(ValueError):
            accounting_for_arm("direct")
        self.assertEqual(
            paired_schedule([11, 12, 13]),
            [
                ("quota", 11, "production_rr"),
                ("quota", 11, "production_actual"),
                ("quota", 12, "production_actual"),
                ("quota", 12, "production_rr"),
                ("quota", 13, "production_rr"),
                ("quota", 13, "production_actual"),
            ],
        )
        with self.assertRaisesRegex(ValueError, "duplicate planned seed"):
            paired_schedule([11, 11])

    def test_actual_config_uses_one_accounting_value_in_toml_and_metadata(self):
        from benchmark_accounting import build_gateway_config, validate_config_provenance

        text, metadata = build_gateway_config(
            arm="production_actual",
            listen_port=31001,
            upstream_port=31002,
            quota=benchmark.QUOTA,
            launched_binary={"path": "/fixture/llmgw", "sha256": "a" * 64, "features": []},
        )
        self.assertIn('accounting = "actual"', text)
        self.assertEqual(metadata["accounting"], "actual")
        self.assertEqual(metadata["snapshot"]["accounting"], "actual")
        validate_config_provenance(metadata, expected_arm="production_actual")

    def test_smoke_provenance_rejects_recomputed_known_quota_config(self):
        from benchmark_accounting import build_gateway_config, validate_config_provenance

        _, metadata = build_gateway_config(
            arm="production_actual",
            listen_port=31001,
            upstream_port=31002,
            quota=benchmark.QUOTA,
            launched_binary={"path": "/fixture/llmgw", "sha256": "a" * 64, "features": []},
        )
        with self.assertRaisesRegex(ValueError, "quota"):
            validate_config_provenance(
                metadata,
                expected_arm="production_actual",
                expected_quota=None,
            )

    def test_config_provenance_rejects_invalid_recorded_loopback_ports(self):
        from benchmark_accounting import build_gateway_config, validate_config_provenance

        _, metadata = build_gateway_config(
            arm="production_actual",
            listen_port=31001,
            upstream_port=31002,
            quota=None,
            launched_binary={"path": "/fixture/llmgw", "sha256": "a" * 64, "features": []},
        )
        for value in (True, 0, 65536):
            with self.subTest(value=value):
                forged = copy.deepcopy(metadata)
                forged["listen_port"] = value
                with self.assertRaisesRegex(ValueError, "port"):
                    validate_config_provenance(
                        forged,
                        expected_arm="production_actual",
                        expected_quota=None,
                    )

    def test_normalized_pair_config_may_differ_only_in_accounting(self):
        from benchmark_accounting import (
            build_gateway_config,
            differing_paths,
            normalize_config_snapshot,
        )

        _, reserved = build_gateway_config(
            arm="production_rr",
            listen_port=31001,
            upstream_port=31002,
            quota=benchmark.QUOTA,
            launched_binary={"path": "/fixture/llmgw", "sha256": "a" * 64, "features": []},
        )
        _, actual = build_gateway_config(
            arm="production_actual",
            listen_port=32001,
            upstream_port=32002,
            quota=benchmark.QUOTA,
            launched_binary={"path": "/fixture/llmgw", "sha256": "a" * 64, "features": []},
        )
        left = normalize_config_snapshot(reserved["snapshot"])
        right = normalize_config_snapshot(actual["snapshot"])
        self.assertEqual(differing_paths(left, right), ["accounting"])
        right["upstream"]["api_base"] = "http://127.0.0.1:<loopback-port>/other/v1?api-version=x"
        self.assertEqual(
            differing_paths(left, right), ["accounting", "upstream.api_base"]
        )

    def test_accounting_identity_has_no_benchmark_binary(self):
        with tempfile.TemporaryDirectory(prefix="accounting-identity-") as tmp:
            binary = Path(tmp) / "llmgw"
            binary.write_bytes(b"ordinary")
            identity = benchmark.identities(binary, None, "accounting")
        self.assertEqual(identity["production"]["features"], [])
        self.assertEqual(identity["benchmark"], {"used": False})
        self.assertIn("scripts/benchmark_accounting.py", identity["sources"])
        self.assertIn("tests/test_benchmark_accounting.py", identity["sources"])
        self.assertFalse(any(name.startswith("docs/") for name in identity["sources"]))


class UsageObservationTests(unittest.IsolatedAsyncioTestCase):
    async def exchange(self, frames):
        server = await one_response_server(frames)
        client = Client(server.sockets[0].getsockname()[1])
        try:
            return await client.exchange("POST", "/", b"{}", {})
        finally:
            await client.close()
            server.close()
            await server.wait_closed()

    async def test_one_numeric_usage_before_done_is_observed(self):
        result = await self.exchange(
            [
                b'{"choices":[{"delta":{"content":"fixture:g"}}]}',
                b'{"choices":[],"usage":{"prompt_tokens":7,"completion_tokens":3}}',
                b"[DONE]",
            ]
        )
        self.assertEqual(
            result["usage"],
            {"status": "observed", "prompt_tokens": 7, "completion_tokens": 3},
        )
        self.assertEqual(result["output"], "fixture:g")
        self.assertIsNotNone(result["terminal_marker_s"])

    async def test_invalid_duplicate_after_terminal_and_missing_usage_are_explicit(self):
        invalid_values = [
            b'{"choices":[],"usage":{"prompt_tokens":true,"completion_tokens":3}}',
            b'{"choices":[],"usage":{"prompt_tokens":"7","completion_tokens":3}}',
            b'{"choices":[],"usage":{"prompt_tokens":-1,"completion_tokens":3}}',
            b'{"choices":[],"usage":{"prompt_tokens":18446744073709551616,"completion_tokens":3}}',
        ]
        for frame in invalid_values:
            with self.subTest(frame=frame):
                result = await self.exchange([frame, b"[DONE]"])
                self.assertEqual(result["usage"]["status"], "invalid")
        duplicate = await self.exchange(
            [
                b'{"choices":[],"usage":{"prompt_tokens":7,"completion_tokens":3}}',
                b'{"choices":[],"usage":{"prompt_tokens":7,"completion_tokens":3}}',
                b"[DONE]",
            ]
        )
        self.assertEqual(duplicate["usage"]["status"], "invalid")
        after = await self.exchange(
            [b"[DONE]", b'{"choices":[],"usage":{"prompt_tokens":7,"completion_tokens":3}}']
        )
        self.assertEqual(after["usage"]["status"], "invalid")
        missing = await self.exchange([b'{"choices":[]}', b"[DONE]"])
        self.assertEqual(missing["usage"], {"status": "missing"})

    async def test_usage_failure_does_not_hide_completed_payload(self):
        row = generation_row()
        server = await one_response_server(
            [
                b'{"choices":[{"delta":{"content":"fixture:g"}}]}',
                b"[DONE]",
            ]
        )
        try:
            result = await request_once(
                row, server.sockets[0].getsockname()[1], "direct", 0
            )
        finally:
            server.close()
            await server.wait_closed()
        self.assertEqual(result["outcome"], "completed")
        self.assertTrue(result["payload_valid"])
        self.assertEqual(result["usage"], {"status": "missing"})


class AccountingRunValidationTests(unittest.TestCase):
    def valid_run(
        self,
        *,
        ended_s=102.0,
        within=True,
        outcome="completed",
        metadata=False,
        arm="production_actual",
    ):
        from benchmark_accounting import build_gateway_config

        row = generation_row()
        if metadata:
            row["metadata"] = True
        body = b"" if metadata else json.dumps(
            {
                "model": "synthetic",
                "messages": [{"role": "user", "content": "x" * row["input_bytes"]}],
                "max_tokens": row["output_reservation"],
                "stream": True,
            },
            separators=(",", ":"),
        ).encode()
        prompt = 0 if metadata else math.ceil(len(body) * row["actual_ratio"])
        completion = 0 if metadata else max(
            1, math.ceil(row["output_reservation"] * row["actual_ratio"])
        )
        _, config = build_gateway_config(
            arm=arm,
            listen_port=31001,
            upstream_port=31002,
            quota=benchmark.QUOTA,
            launched_binary={"path": "/fixture/llmgw", "sha256": "a" * 64, "features": []},
        )
        usage = (
            {"status": "not_applicable"}
            if metadata and outcome == "completed"
            else {"status": "observed", "prompt_tokens": prompt, "completion_tokens": completion}
            if outcome == "completed"
            else {"status": "not_observed"}
        )
        terminal = {
            "id": row["id"],
            "outcome": outcome,
            "ended_s": ended_s,
            "within_measurement": within,
            "elapsed_ms": 1,
            "scheduling_lag_ms": 0,
            "usage": usage,
        }
        if outcome == "completed":
            terminal["payload_valid"] = True
        attempts = [] if outcome != "completed" else [
            {
                "id": row["id"] + "/1",
                "ingress_id": row["id"],
                "outcome": "completed",
                "body_bytes": len(body),
                "actual_cost_fixture_units": prompt + completion,
            }
        ]
        run = {
            "id": f"quota-seed1-{arm}",
            "phase": "quota",
            "seed": 1,
            "arm": arm,
            "submitted": [row],
            "outcomes": [terminal],
            "attempts": attempts,
            "start_monotonic_s": 100.0,
            "measurement_duration_s": 2.0,
            "quota_windows": 5,
            "mock_quota": benchmark.QUOTA,
            "config": config,
            "gateway_started_attempts": len(attempts),
        }
        run["initial_status"] = worker_status(config)
        run["final_status"] = worker_status(
            config, requests=4, upstream_attempts=len(attempts)
        )
        return run

    def test_cutoff_is_recomputed_and_exact_boundary_is_included(self):
        from benchmark_accounting import validate_accounting_run

        validate_accounting_run(self.valid_run())
        after = self.valid_run(ended_s=102.000001, within=False)
        validate_accounting_run(after)
        forged = self.valid_run(ended_s=102.000001, within=True)
        with self.assertRaisesRegex(ValueError, "within_measurement"):
            validate_accounting_run(forged)

    def test_nonfinite_measurement_timestamps_are_rejected(self):
        from benchmark_accounting import validate_accounting_run

        cases = [
            ("start_monotonic_s", float("nan"), False),
            ("start_monotonic_s", float("inf"), True),
            ("start_monotonic_s", float("-inf"), False),
            ("measurement_duration_s", float("nan"), False),
            ("measurement_duration_s", float("inf"), True),
            ("measurement_duration_s", float("-inf"), False),
            ("ended_s", float("nan"), False),
            ("ended_s", float("inf"), False),
            ("ended_s", float("-inf"), True),
        ]
        for field, value, within in cases:
            with self.subTest(field=field, value=value):
                run = self.valid_run(within=within)
                if field == "ended_s":
                    run["outcomes"][0][field] = value
                else:
                    run[field] = value
                with self.assertRaisesRegex(ValueError, "finite"):
                    validate_accounting_run(run)

    def test_persisted_initial_and_final_worker_status_match_config(self):
        from benchmark_accounting import build_gateway_config, validate_accounting_run

        validate_accounting_run(self.valid_run())
        for field in ("initial_status", "final_status"):
            with self.subTest(field=field, case="missing"):
                missing = self.valid_run()
                missing.pop(field)
                with self.assertRaisesRegex(ValueError, field):
                    validate_accounting_run(missing)
            with self.subTest(field=field, case="accounting"):
                mismatch = self.valid_run()
                mismatch[field]["admission"]["accounting"] = "reserved"
                with self.assertRaisesRegex(ValueError, field + ".*accounting"):
                    validate_accounting_run(mismatch)
            with self.subTest(field=field, case="fingerprint"):
                mismatch = self.valid_run()
                mismatch[field]["identity"]["fingerprint"] = "0" * 64
                with self.assertRaisesRegex(ValueError, field + ".*fingerprint"):
                    validate_accounting_run(mismatch)

        relabelled = self.valid_run(arm="production_rr")
        relabelled.update(id="quota-seed1-production_actual", arm="production_actual")
        old = relabelled["config"]
        _, relabelled["config"] = build_gateway_config(
            arm="production_actual",
            listen_port=old["listen_port"],
            upstream_port=old["upstream_port"],
            quota=old["quota"],
            launched_binary=old["launched_binary"],
        )
        with self.assertRaisesRegex(
            ValueError, "initial_status.*(accounting|fingerprint)"
        ):
            validate_accounting_run(relabelled)

    def test_completed_generation_usage_is_linked_to_submitted_and_attempt(self):
        from benchmark_accounting import validate_accounting_run

        validate_accounting_run(self.valid_run())
        missing = self.valid_run()
        missing["outcomes"][0]["usage"] = {"status": "missing"}
        with self.assertRaisesRegex(ValueError, "usage"):
            validate_accounting_run(missing)
        altered = self.valid_run()
        altered["outcomes"][0]["usage"]["prompt_tokens"] += 1
        with self.assertRaisesRegex(ValueError, "usage"):
            validate_accounting_run(altered)

        for value in (1.0, True, "1", -1, 2**64):
            with self.subTest(value=value):
                invalid = self.valid_run()
                invalid["outcomes"][0]["usage"]["prompt_tokens"] = value
                with self.assertRaisesRegex(ValueError, "usage"):
                    validate_accounting_run(invalid)

    def test_metadata_and_noncompleted_outcomes_need_no_usage(self):
        from benchmark_accounting import validate_accounting_run

        validate_accounting_run(self.valid_run(metadata=True))
        validate_accounting_run(
            self.valid_run(outcome="cancelled", ended_s=101, within=True)
        )

    def test_paired_summary_separates_fixed_cutoff_drain_and_latency(self):
        from benchmark_accounting import paired_summary

        reserved = self.valid_run(
            arm="production_rr", ended_s=400.000001, within=False
        )
        actual = self.valid_run(arm="production_actual", ended_s=400.0, within=True)
        reserved["measurement_duration_s"] = 300
        actual["measurement_duration_s"] = 300
        summary = paired_summary([reserved, actual], [1, 2], "quota")
        self.assertEqual(summary["completed_pairs"], 1)
        self.assertEqual(summary["planned_pairs"], 2)
        self.assertEqual(
            summary["pairs"][0][
                "actual_minus_reserved_within_measurement_completed"
            ],
            1,
        )
        self.assertEqual(
            summary["pairs"][0]["arms"]["production_rr"][
                "post_measurement_drain_completed"
            ],
            1,
        )
        self.assertEqual(
            summary["pairs"][0]["arms"]["production_actual"][
                "completion_ms_success_only"
            ]["n"],
            1,
        )
        self.assertEqual(
            summary["interpretation_scope"],
            "one_pair_checkpoint_only_no_multi_seed_conclusion",
        )
        forged = self.valid_run(
            arm="production_actual", ended_s=400.000001, within=True
        )
        forged["measurement_duration_s"] = 300
        with self.assertRaisesRegex(ValueError, "within_measurement"):
            paired_summary([reserved, forged], [1], "quota")
        with self.assertRaisesRegex(ValueError, "duplicate"):
            paired_summary([reserved, actual, copy.deepcopy(actual)], [1], "quota")
        with self.assertRaisesRegex(ValueError, "duplicate planned seed"):
            paired_summary([reserved, actual], [1, 1], "quota")
        wrong_duration = [copy.deepcopy(reserved), copy.deepcopy(actual)]
        for run in wrong_duration:
            run["measurement_duration_s"] = 299
            run["outcomes"][0]["ended_s"] = 399
            run["outcomes"][0]["within_measurement"] = True
        with self.assertRaisesRegex(ValueError, "duration"):
            paired_summary(wrong_duration, [1], "quota")


class AccountingCliBoundaryTests(unittest.IsolatedAsyncioTestCase):
    async def test_invalid_mode_combinations_reject_before_output_creation(self):
        cases = [
            dict(mode="accounting", phase="all", smoke=False, pilot_only=False, windows=5),
            dict(mode="baseline", phase="quota", smoke=False, pilot_only=True, windows=5),
            dict(mode="accounting", phase="quota", smoke=True, pilot_only=True, windows=5),
            dict(mode="accounting", phase="quota", smoke=False, pilot_only=False, windows=4),
        ]
        with tempfile.TemporaryDirectory(prefix="accounting-cli-red-") as tmp:
            root = Path(tmp)
            binary = root / "binary"
            binary.write_bytes(b"identity")
            for index, values in enumerate(cases):
                output = root / f"case-{index}" / "manifest.json"
                args = argparse.Namespace(
                    binary=binary,
                    reference_binary=root / "unused-reference",
                    seeds="1",
                    output=output,
                    **values,
                )
                with self.assertRaises(ValueError):
                    await benchmark.main(args)
                self.assertFalse(output.parent.exists())

    async def test_duplicate_accounting_seeds_reject_before_output_creation(self):
        with tempfile.TemporaryDirectory(prefix="accounting-duplicate-seeds-") as tmp:
            root = Path(tmp)
            binary = root / "binary"
            binary.write_bytes(b"identity")
            output = root / "result" / "manifest.json"
            args = argparse.Namespace(
                binary=binary,
                reference_binary=root / "unused-reference",
                seeds="1,1",
                windows=5,
                output=output,
                smoke=False,
                phase="quota",
                mode="accounting",
                pilot_only=False,
            )
            with mock.patch.object(
                benchmark, "run_arm", side_effect=AssertionError("spawned")
            ):
                with self.assertRaisesRegex(ValueError, "unique seeds"):
                    await benchmark.main(args)
            self.assertFalse(output.parent.exists())

    def args(self, root, *, pilot_only):
        binary = root / "llmgw"
        if not binary.exists():
            binary.write_bytes(b"ordinary product")
        return argparse.Namespace(
            binary=binary,
            reference_binary=root / "unused-reference",
            seeds="1,2",
            windows=5,
            output=root / "accounting.json",
            smoke=False,
            phase="quota",
            mode="accounting",
            pilot_only=pilot_only,
        )

    def fake_run(self, binary, calls, fail_arm=None):
        from benchmark_accounting import build_gateway_config

        async def run_arm(arm, seed, windows, candidate, reference, phase, directory):
            calls.append((phase, seed, arm))
            if arm == fail_arm:
                raise RuntimeError("injected second arm failure")
            rows = benchmark.workload(seed, windows)
            attempts = []
            outcomes = []
            for row in rows:
                metadata = row.get("metadata", False)
                body = b"" if metadata else encode(
                    {
                        "model": "synthetic",
                        "messages": [{"role": "user", "content": "x" * row["input_bytes"]}],
                        "max_tokens": row["output_reservation"],
                        "stream": True,
                    }
                )
                prompt = 0 if metadata else math.ceil(len(body) * row["actual_ratio"])
                completion = 0 if metadata else max(
                    1, math.ceil(row["output_reservation"] * row["actual_ratio"])
                )
                attempts.append(
                    {
                        "id": row["id"] + "/1",
                        "ingress_id": row["id"],
                        "outcome": "completed",
                        "body_bytes": len(body),
                        "actual_cost_fixture_units": prompt + completion,
                    }
                )
                outcomes.append(
                    {
                        "id": row["id"],
                        "outcome": "completed",
                        "payload_valid": True,
                        "ended_s": 101.0,
                        "within_measurement": True,
                        "elapsed_ms": 1,
                        "scheduling_lag_ms": 0,
                        "usage": {"status": "not_applicable"}
                        if metadata
                        else {
                            "status": "observed",
                            "prompt_tokens": prompt,
                            "completion_tokens": completion,
                        },
                    }
                )
            launched = {
                "path": str(candidate.resolve()),
                "sha256": hashlib.sha256(candidate.read_bytes()).hexdigest(),
                "features": [],
            }
            _, config = build_gateway_config(
                arm=arm,
                listen_port=31001 + seed,
                upstream_port=32001 + seed,
                quota=benchmark.QUOTA,
                launched_binary=launched,
            )
            run_id = f"{phase}-seed{seed}-{arm}"
            run = {
                "id": run_id,
                "phase": phase,
                "seed": seed,
                "arm": arm,
                "submitted": rows,
                "outcomes": outcomes,
                "attempts": attempts,
                "start_monotonic_s": 100.0,
                "measurement_duration_s": 300,
                "quota_windows": 5,
                "mock_quota": benchmark.QUOTA,
                "config": config,
                "gateway_started_attempts": len(attempts),
                "owned_processes_cleaned": True,
            }
            run["initial_status"] = worker_status(config)
            run["final_status"] = worker_status(
                config, requests=len(outcomes) + 3, upstream_attempts=len(attempts)
            )
            run["summary"] = benchmark.aggregate(run)
            benchmark.write_json(directory / (run_id + ".json"), run)
            (directory / (run_id + ".events.jsonl")).write_text(
                json.dumps({"event": "validated"}) + "\n"
            )
            return run

        return run_arm

    async def test_pilot_records_full_schedule_then_resume_reuses_pair(self):
        with tempfile.TemporaryDirectory(prefix="accounting-pilot-") as tmp:
            root = Path(tmp)
            args = self.args(root, pilot_only=True)
            calls = []
            with mock.patch.object(
                benchmark, "run_arm", self.fake_run(args.binary, calls)
            ):
                await benchmark.main(args)
            first = json.loads(args.output.read_text())
            self.assertEqual(
                calls,
                [
                    ("quota", 1, "production_rr"),
                    ("quota", 1, "production_actual"),
                ],
            )
            self.assertEqual(first["mode"], "accounting")
            self.assertEqual(len(first["schedule"]), 4)
            self.assertEqual(first["status"], "accounting_pilot_completed")
            self.assertEqual(first["completed_pairs"], 1)
            self.assertEqual(first["planned_pairs"], 2)
            self.assertFalse(first["matrix_complete"])
            self.assertEqual(first["identity"]["benchmark"], {"used": False})
            for entry in first["runs"]:
                run = json.loads(Path(entry["path"]).read_text())
                self.assertEqual(
                    entry["provenance"],
                    {
                        "accounting": run["config"]["accounting"],
                        "raw_toml_sha256": run["config"]["raw_toml_sha256"],
                        "launched_binary": run["config"]["launched_binary"],
                    },
                )

            calls.clear()
            args.pilot_only = False
            with mock.patch.object(
                benchmark, "run_arm", self.fake_run(args.binary, calls)
            ):
                await benchmark.main(args)
            completed = json.loads(args.output.read_text())
            self.assertEqual(
                calls,
                [
                    ("quota", 2, "production_actual"),
                    ("quota", 2, "production_rr"),
                ],
            )
            self.assertEqual(completed["status"], "completed_accounting_matrix")
            self.assertTrue(completed["matrix_complete"])
            self.assertEqual(completed["completed_pairs"], 2)
            self.assertEqual(len(completed["runs"]), 4)

    async def test_single_seed_pilot_is_checkpoint_then_full_resume_reuses_pair(self):
        with tempfile.TemporaryDirectory(prefix="accounting-single-pilot-") as tmp:
            root = Path(tmp)
            args = self.args(root, pilot_only=True)
            args.seeds = "1"
            calls = []
            with mock.patch.object(
                benchmark, "run_arm", self.fake_run(args.binary, calls)
            ):
                await benchmark.main(args)
            pilot = json.loads(args.output.read_text())
            self.assertEqual(pilot["status"], "accounting_pilot_completed")
            self.assertEqual(pilot["completed_pairs"], 1)
            self.assertEqual(pilot["planned_pairs"], 1)
            self.assertFalse(pilot["matrix_complete"])

            calls.clear()
            args.pilot_only = False
            with mock.patch.object(
                benchmark, "run_arm", self.fake_run(args.binary, calls)
            ):
                await benchmark.main(args)
            completed = json.loads(args.output.read_text())
            self.assertEqual(calls, [])
            self.assertEqual(completed["status"], "completed_accounting_matrix")
            self.assertTrue(completed["matrix_complete"])
            self.assertEqual(completed["completed_pairs"], 1)

    async def test_forged_accounting_or_snapshot_rejects_before_next_spawn(self):
        with tempfile.TemporaryDirectory(prefix="accounting-forgery-") as tmp:
            root = Path(tmp)
            args = self.args(root, pilot_only=True)
            calls = []
            with mock.patch.object(
                benchmark, "run_arm", self.fake_run(args.binary, calls)
            ):
                await benchmark.main(args)
            manifest = json.loads(args.output.read_text())
            run_path = Path(manifest["runs"][0]["path"])
            original_run = json.loads(run_path.read_text())

            forged = copy.deepcopy(original_run)
            forged["config"]["accounting"] = "actual"
            benchmark.write_json(run_path, forged)
            manifest["runs"][0]["sha256"] = benchmark.digest(run_path)
            benchmark.write_json(args.output, manifest)
            calls.clear()
            args.pilot_only = False
            with mock.patch.object(
                benchmark, "run_arm", self.fake_run(args.binary, calls)
            ):
                with self.assertRaisesRegex(ValueError, "accounting"):
                    await benchmark.main(args)
            self.assertEqual(calls, [])

            benchmark.write_json(run_path, original_run)
            manifest["runs"][0]["sha256"] = benchmark.digest(run_path)
            forged = json.loads(run_path.read_text())
            forged["config"]["snapshot"]["upstream"]["api_base"] = (
                "http://127.0.0.1:32002/other/v1?api-version=fixture"
            )
            forged["config"]["raw_toml_sha256"] = "b" * 64
            benchmark.write_json(run_path, forged)
            manifest["runs"][0]["sha256"] = benchmark.digest(run_path)
            benchmark.write_json(args.output, manifest)
            with mock.patch.object(
                benchmark, "run_arm", self.fake_run(args.binary, calls)
            ):
                with self.assertRaisesRegex(ValueError, "TOML"):
                    await benchmark.main(args)
            self.assertEqual(calls, [])

            benchmark.write_json(run_path, original_run)
            manifest["runs"][0]["sha256"] = benchmark.digest(run_path)
            forged = json.loads(run_path.read_text())
            forged["config"]["snapshot"]["models"][0]["max_output_tokens"] = 8192
            forged["config"]["raw_toml_sha256"] = "c" * 64
            benchmark.write_json(run_path, forged)
            manifest["runs"][0]["sha256"] = benchmark.digest(run_path)
            benchmark.write_json(args.output, manifest)
            with mock.patch.object(
                benchmark, "run_arm", self.fake_run(args.binary, calls)
            ):
                with self.assertRaisesRegex(ValueError, "TOML"):
                    await benchmark.main(args)
            self.assertEqual(calls, [])

    async def test_rehashed_equal_float_usage_rejects_before_next_spawn(self):
        with tempfile.TemporaryDirectory(prefix="accounting-float-resume-") as tmp:
            root = Path(tmp)
            args = self.args(root, pilot_only=True)
            calls = []
            with mock.patch.object(
                benchmark, "run_arm", self.fake_run(args.binary, calls)
            ):
                await benchmark.main(args)
            manifest = json.loads(args.output.read_text())
            run_path = Path(manifest["runs"][0]["path"])
            forged = json.loads(run_path.read_text())
            terminal = next(
                row
                for row in forged["outcomes"]
                if row["outcome"] == "completed"
                and row["usage"]["status"] == "observed"
            )
            terminal["usage"]["prompt_tokens"] = float(
                terminal["usage"]["prompt_tokens"]
            )
            benchmark.write_json(run_path, forged)
            manifest["runs"][0]["sha256"] = benchmark.digest(run_path)
            benchmark.write_json(args.output, manifest)

            calls.clear()
            args.pilot_only = False
            with mock.patch.object(
                benchmark, "run_arm", self.fake_run(args.binary, calls)
            ):
                with self.assertRaisesRegex(ValueError, "usage"):
                    await benchmark.main(args)
            self.assertEqual(calls, [])

    async def test_rehashed_runtime_status_forgery_rejects_before_next_spawn(self):
        from benchmark_accounting import build_gateway_config

        def missing_initial(actual, reserved):
            actual.pop("initial_status")

        def mismatched_accounting(actual, reserved):
            actual["initial_status"]["admission"]["accounting"] = "reserved"

        def mismatched_fingerprint(actual, reserved):
            actual["final_status"]["identity"]["fingerprint"] = "0" * 64

        def relabel_reserved(actual, reserved):
            candidate = copy.deepcopy(reserved)
            candidate.update(id=actual["id"], arm="production_actual")
            old = candidate["config"]
            _, candidate["config"] = build_gateway_config(
                arm="production_actual",
                listen_port=old["listen_port"],
                upstream_port=old["upstream_port"],
                quota=old["quota"],
                launched_binary=old["launched_binary"],
            )
            actual.clear()
            actual.update(candidate)

        for name, mutate in (
            ("missing_initial", missing_initial),
            ("mismatched_accounting", mismatched_accounting),
            ("mismatched_fingerprint", mismatched_fingerprint),
            ("relabel_reserved", relabel_reserved),
        ):
            with self.subTest(name=name), tempfile.TemporaryDirectory(
                prefix="accounting-runtime-resume-"
            ) as tmp:
                root = Path(tmp)
                args = self.args(root, pilot_only=True)
                calls = []
                with mock.patch.object(
                    benchmark, "run_arm", self.fake_run(args.binary, calls)
                ):
                    await benchmark.main(args)
                manifest = json.loads(args.output.read_text())
                reserved_entry = next(
                    entry for entry in manifest["runs"] if entry["id"].endswith("production_rr")
                )
                actual_entry = next(
                    entry for entry in manifest["runs"] if entry["id"].endswith("production_actual")
                )
                reserved = json.loads(Path(reserved_entry["path"]).read_text())
                actual_path = Path(actual_entry["path"])
                actual = json.loads(actual_path.read_text())
                mutate(actual, reserved)
                benchmark.write_json(actual_path, actual)
                actual_entry["sha256"] = benchmark.digest(actual_path)
                actual_entry["summary"] = actual["summary"]
                actual_entry["provenance"] = {
                    key: actual["config"][key]
                    for key in ("accounting", "raw_toml_sha256", "launched_binary")
                }
                benchmark.write_json(args.output, manifest)

                calls.clear()
                args.pilot_only = False
                with mock.patch.object(
                    benchmark, "run_arm", self.fake_run(args.binary, calls)
                ):
                    with self.assertRaisesRegex(ValueError, "status|accounting|fingerprint"):
                        await benchmark.main(args)
                self.assertEqual(calls, [])

    async def test_second_arm_failure_never_marks_pilot_complete(self):
        with tempfile.TemporaryDirectory(prefix="accounting-partial-") as tmp:
            root = Path(tmp)
            args = self.args(root, pilot_only=True)
            calls = []
            with mock.patch.object(
                benchmark,
                "run_arm",
                self.fake_run(args.binary, calls, fail_arm="production_actual"),
            ):
                with self.assertRaisesRegex(RuntimeError, "second arm"):
                    await benchmark.main(args)
            manifest = json.loads(args.output.read_text())
            self.assertEqual(len(manifest["runs"]), 1)
            self.assertEqual(manifest["status"], "in_progress")
            self.assertNotEqual(manifest.get("completed_pairs"), 1)

    async def test_changed_mode_schedule_seeds_or_windows_reject_before_spawn(self):
        with tempfile.TemporaryDirectory(prefix="accounting-resume-identity-") as tmp:
            root = Path(tmp)
            args = self.args(root, pilot_only=True)
            calls = []
            with mock.patch.object(
                benchmark, "run_arm", self.fake_run(args.binary, calls)
            ):
                await benchmark.main(args)
            before = args.output.read_bytes()
            (root / "unused-reference").write_bytes(b"reference")
            cases = [
                {"mode": "baseline", "pilot_only": False},
                {"seeds": "1,3", "pilot_only": False},
                {"windows": 4, "pilot_only": False},
            ]
            for changes in cases:
                with self.subTest(changes=changes):
                    changed = copy.copy(args)
                    for name, value in changes.items():
                        setattr(changed, name, value)
                    calls.clear()
                    with mock.patch.object(
                        benchmark, "run_arm", self.fake_run(args.binary, calls)
                    ):
                        with self.assertRaises(ValueError):
                            await benchmark.main(changed)
                    self.assertEqual(calls, [])
                    self.assertEqual(args.output.read_bytes(), before)


class StartupCancellationTests(unittest.IsolatedAsyncioTestCase):
    async def test_benchmark_arm_keeps_its_existing_health_only_startup_contract(self):
        class FixtureProcess:
            def __init__(self, pid, returncode):
                self.pid = pid
                self.returncode = returncode

            async def communicate(self):
                return json.dumps({"state_directory": str(state)}).encode(), b""

            async def wait(self):
                return self.returncode

            def terminate(self):
                self.returncode = -15

            def kill(self):
                self.returncode = -9

        for arm, expected_policy in [("benchmark_rr", "rr"), ("benchmark_backfill", "backfill")]:
            with tempfile.TemporaryDirectory(prefix="accounting-benchmark-arm-") as tmp:
                root = Path(tmp)
                state = root / "state"
                binary = root / "llmgw"
                reference = root / "bench_gateway"
                binary.write_bytes(b"ordinary")
                reference.write_bytes(b"bench")
                doctor = FixtureProcess(101, 0)
                worker = FixtureProcess(102, None)
                controls = []

                async def fixture_control(port, path="status", method="GET"):
                    controls.append((path, method))
                    return {"status": "ok"}

                with mock.patch.object(
                    benchmark.asyncio,
                    "create_subprocess_exec",
                    side_effect=[doctor, worker],
                ) as spawned, mock.patch.object(
                    benchmark, "control", fixture_control
                ), mock.patch(
                    "benchmark_accounting.validate_written_config",
                    side_effect=AssertionError("ordinary written-config guard called"),
                ), mock.patch(
                    "benchmark_accounting.validate_runtime_config",
                    side_effect=AssertionError("ordinary runtime guard called"),
                ):
                    proc, _, metadata = await benchmark.start_gateway(
                        arm,
                        binary,
                        reference,
                        9,
                        None,
                        root,
                        lambda event: None,
                    )
                self.assertEqual(spawned.call_args_list[1].args[-2:], ("--policy", expected_policy))
                self.assertIs(proc, worker)
                self.assertEqual(metadata["launched_binary"]["features"], ["bench-harness"])
                self.assertEqual(controls, [("health", "GET")])
                await benchmark.abort_startup(worker)

    async def test_builder_toml_metadata_mismatch_rejects_before_spawn(self):
        from benchmark_accounting import build_gateway_config

        binary = PRODUCT / "target/native/release/llmgw"
        if not binary.is_file():
            self.skipTest("accepted native release binary unavailable")

        def wrong_actual_toml(**kwargs):
            text, metadata = build_gateway_config(**kwargs)
            return text.replace(
                'accounting = "actual"', 'accounting = "reserved"'
            ), metadata

        with tempfile.TemporaryDirectory(prefix="accounting-builder-mismatch-") as tmp:
            with mock.patch(
                "benchmark_accounting.build_gateway_config", wrong_actual_toml
            ), mock.patch.object(
                benchmark.asyncio,
                "create_subprocess_exec",
                side_effect=AssertionError("subprocess spawned"),
            ):
                with self.assertRaisesRegex(ValueError, "written TOML"):
                    await benchmark.start_gateway(
                        "production_actual",
                        binary,
                        None,
                        9,
                        benchmark.QUOTA,
                        tmp,
                        lambda event: None,
                    )

    async def test_written_toml_mutation_rejects_before_spawn(self):
        binary = PRODUCT / "target/native/release/llmgw"
        if not binary.is_file():
            self.skipTest("accepted native release binary unavailable")
        original_write_text = Path.write_text

        def wrong_write(path, text, *args, **kwargs):
            if path.name == "gateway.toml":
                text = text.replace(
                    'accounting = "actual"', 'accounting = "reserved"'
                )
            return original_write_text(path, text, *args, **kwargs)

        with tempfile.TemporaryDirectory(prefix="accounting-write-mismatch-") as tmp:
            with mock.patch.object(Path, "write_text", wrong_write), mock.patch.object(
                benchmark.asyncio,
                "create_subprocess_exec",
                side_effect=AssertionError("subprocess spawned"),
            ):
                with self.assertRaisesRegex(ValueError, "written TOML"):
                    await benchmark.start_gateway(
                        "production_actual",
                        binary,
                        None,
                        9,
                        benchmark.QUOTA,
                        tmp,
                        lambda event: None,
                    )

    async def test_runtime_accounting_mismatch_reaps_owned_child(self):
        binary = PRODUCT / "target/native/release/llmgw"
        if not binary.is_file():
            self.skipTest("accepted native release binary unavailable")
        original_control = benchmark.control
        events = []

        async def wrong_status(port, path="status", method="GET"):
            result = await original_control(port, path, method)
            if path == "status":
                result["admission"]["accounting"] = "reserved"
            return result

        with tempfile.TemporaryDirectory(prefix="accounting-runtime-mismatch-") as tmp:
            returned = None
            with mock.patch.object(benchmark, "control", wrong_status):
                try:
                    returned = await benchmark.start_gateway(
                        "production_actual",
                        binary,
                        None,
                        9,
                        benchmark.QUOTA,
                        tmp,
                        events.append,
                    )
                except ValueError as error:
                    self.assertRegex(str(error), "runtime accounting")
                else:
                    await benchmark.stop_gateway(returned[0], returned[1])
                    self.fail("runtime accounting mismatch accepted")
            pid = next(
                event["pid"] for event in events if event["event"] == "gateway_started"
            )
            with self.assertRaises(ProcessLookupError):
                os.kill(pid, 0)

    async def test_real_gateway_startup_cancellation_reaps_owned_child(self):
        binary = PRODUCT / "target/native/release/llmgw"
        if not binary.is_file():
            self.skipTest("accepted native release binary unavailable")
        entered = asyncio.Event()
        never = asyncio.Event()
        events = []

        async def held_control(port, path="status", method="GET"):
            if path == "health":
                entered.set()
                await never.wait()
            raise AssertionError("unexpected control call")

        with tempfile.TemporaryDirectory(prefix="accounting-start-cancel-") as tmp:
            with mock.patch.object(benchmark, "control", held_control):
                task = asyncio.create_task(
                    benchmark.start_gateway(
                        "production_actual",
                        binary,
                        None,
                        9,
                        benchmark.QUOTA,
                        tmp,
                        events.append,
                    )
                )
                await asyncio.wait_for(entered.wait(), 5)
                pid = next(
                    event["pid"]
                    for event in events
                    if event["event"] == "gateway_started"
                )
                task.cancel()
                await asyncio.sleep(0)
                task.cancel()
                with self.assertRaises(asyncio.CancelledError):
                    await task
                with self.assertRaises(ProcessLookupError):
                    os.kill(pid, 0)


if __name__ == "__main__":
    unittest.main()
