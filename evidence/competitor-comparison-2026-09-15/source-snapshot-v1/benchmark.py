#!/usr/bin/env python3
"""Native synthetic benchmark. No external endpoint, client config or model access."""
import argparse
import copy

def validate_run(run):
    submitted = [r["id"] for r in run["submitted"]]
    terminal = [r["id"] for r in run["outcomes"]]
    attempts = [r["id"] for r in run["attempts"]]
    if len(set(submitted)) != len(submitted):
        raise ValueError("duplicate submitted ID")
    if len(set(terminal)) != len(terminal) or set(terminal) != set(submitted):
        raise ValueError("terminal denominator mismatch")
    if len(set(attempts)) != len(attempts):
        raise ValueError("duplicate attempt ID")
    if any(r["ingress_id"] not in submitted for r in run["attempts"]):
        raise ValueError("unknown attempt ingress ID")
    if any(a.get("outcome") not in {"completed", "rejected", "disconnected"} for a in run["attempts"]):
        raise ValueError("pending or unknown upstream attempt outcome")
    if any(r["outcome"]=="completed" and r.get("payload_valid") is not True for r in run["outcomes"]):
        raise ValueError("completion lacks validated payload and terminal evidence")
    if any(r["outcome"] not in {"completed", "rejected", "error", "timeout", "cancelled"} for r in run["outcomes"]):
        raise ValueError("nonterminal outcome")

def self_check():
    valid = {"submitted": [{"id": "a"}, {"id": "b"}],
             "outcomes": [{"id": "a", "outcome": "completed", "payload_valid": True}, {"id": "b", "outcome": "timeout"}],
             "attempts": [{"id": "a/1", "ingress_id": "a", "outcome": "completed"}]}
    validate_run(valid)
    for name, change in [
        ("missing denominator", lambda r: r["outcomes"].pop()),
        ("duplicate attempts", lambda r: r["attempts"].append(copy.deepcopy(r["attempts"][0]))),
        ("unknown attempt ingress", lambda r: r["attempts"][0].update(ingress_id="unknown")),
        ("pending attempt", lambda r: r["attempts"][0].update(outcome="pending")),
        ("HTTP200 without payload", lambda r: r["outcomes"][0].pop("payload_valid")),
        ("duplicate terminal", lambda r: r["outcomes"].append(copy.deepcopy(r["outcomes"][0]))),
    ]:
        broken = copy.deepcopy(valid)
        change(broken)
        try:
            validate_run(broken)
        except ValueError:
            print(f"REJECTED {name}")
        else:
            raise AssertionError(f"accepted {name}")
    print("validation checks PASS")

import asyncio
import collections
import hashlib
import json
import math
import os
import platform
import random
import signal
import statistics
import sys
import tempfile
import time
from pathlib import Path

ARMS = ("direct", "production_rr", "benchmark_fifo", "benchmark_rr")
WINDOW = 60
QUOTA = {"rpm": 16, "tpm": 6000}
CAP = 2
ROOTS = 4
HERE = Path(__file__).resolve().parent.parent

def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def write_json(path, value):
    temp = path.with_suffix(path.suffix + ".tmp")
    temp.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n")
    os.replace(temp, path)

def distribution(values):
    ordered = sorted(values)
    def percentile(q):
        return ordered[min(len(ordered)-1, math.ceil(q*len(ordered))-1)] if ordered else None
    return {"n": len(ordered), "p50": percentile(.5), "p95": percentile(.95),
            "p99": percentile(.99), "max": max(ordered) if ordered else None}

def aggregate(run):
    submitted = {r["id"]: r for r in run["submitted"]}
    groups = {}
    for outcome in run["outcomes"]:
        req = submitted[outcome["id"]]
        for key in ("all", "length:"+req["length"], "workload:"+req["workload"], "root:"+str(req["root"]), "cost:"+req["cost_case"]):
            groups.setdefault(key, []).append(outcome)
    result = {}
    for name, rows in groups.items():
        within = [r for r in rows if r.get("within_measurement", True)]
        result[name] = {"submitted": len(rows), "within_measurement_terminal":dict(collections.Counter(r["outcome"] for r in within)),
                       "within_measurement_completed":sum(r["outcome"]=="completed" for r in within),
                       "post_measurement_drain_outcomes":dict(collections.Counter(r["outcome"] for r in rows if not r.get("within_measurement",True))), "outcomes": dict(collections.Counter(r["outcome"] for r in rows)),
                       "completion_ms_success_only": distribution([r["elapsed_ms"] for r in rows if r["outcome"] == "completed"]),
                       "terminal_ms_all_outcomes": distribution([r["elapsed_ms"] for r in rows]),
                       "first_output_delta_ms_success_only": distribution([r["first_output_delta_ms"] for r in rows if r["outcome"] == "completed" and r.get("first_output_delta_ms") is not None]),
                       "send_to_mock_receive_ms_mixed_ingress_queue_network": distribution([r["send_to_mock_receive_ms"] for r in rows if r.get("send_to_mock_receive_ms") is not None]),
                       "scheduling_lag_ms": distribution([r["scheduling_lag_ms"] for r in rows])}
    workflow = collections.defaultdict(list)
    for r in run["outcomes"]:
        workflow[submitted[r["id"]]["workflow_id"]].append(r)
    result["synthetic_pair_workflows"] = {"submitted": len(workflow), "completed": sum(len(v)==2 and all(x["outcome"]=="completed" for x in v) for v in workflow.values()), "completed_within_measurement":sum(len(v)==2 and all(x["outcome"]=="completed" and x.get("within_measurement",True) for x in v) for v in workflow.values()), "definition": "two predetermined independent fixture responses both validated; not an agent task"}
    result["retry_amplification"] = {"upstream_attempts": len(run["attempts"]), "ingress_submitted": len(submitted), "mock_received_attempts_per_ingress": len(run["attempts"])/len(submitted) if submitted else None, "retried_ingress": sum(v>1 for v in collections.Counter(a["ingress_id"] for a in run["attempts"]).values())}
    gateway_started = run.get("gateway_started_attempts")
    result["retry_amplification"].update(gateway_started_attempts=gateway_started, gateway_started_per_ingress=gateway_started/len(submitted) if gateway_started is not None and submitted else None, gateway_started_without_mock_receive=gateway_started-len(run["attempts"]) if gateway_started is not None else None)
    return result

def workload(seed, windows, smoke=False):
    rng = random.Random(seed)
    rows = []
    for window in range(windows):
        for name in ("burst", "mixed_lengths", "shared_quota", "cancellation"):
            spec = json.loads((HERE/"fixtures/workloads"/(name+".json")).read_text())
            for index, template in enumerate(spec["requests"]):
                row = dict(template)
                row.update(id=f"w{window}-{name}-{index}", window=window, workload=name,
                           workflow_id=f"w{window}-{name}-pair{index//2}",
                           offset_s=window*WINDOW+spec["window_offset_s"]+template["offset_ms"]/1000+rng.uniform(0,.01),
                           length="long" if template["input_bytes"]>=1000 else "short",
                           cost_case="matching" if template["actual_ratio"]==1 else "overestimated")
                if smoke:
                    row["offset_s"] /= 20
                rows.append(row)
    return rows

async def owned_resources(pid):
    proc = await asyncio.create_subprocess_exec("ps", "-o", "rss=,time=", "-p", str(pid), stdout=asyncio.subprocess.PIPE)
    output, _ = await proc.communicate()
    fields = output.decode().split()
    if len(fields) != 2:
        return None
    cpu = 0.0
    for field in fields[1].split(":"):
        cpu = cpu*60+float(field)
    return {"monotonic_s": time.monotonic(), "rss_bytes": int(fields[0])*1024, "cpu_seconds": cpu}

async def control(port, path="status", method="GET"):
    from benchmark_http import Client
    client = Client(port)
    try:
        response = await asyncio.wait_for(client.exchange(method, "/_llmgw/"+path, b"", {"x-llmgw-control-token": "synthetic-benchmark-control"}),2)
        if response["status"] != 200:
            raise RuntimeError("control failed")
        return json.loads(response["body"])
    finally:
        await client.close()

async def abort_startup(proc):
    """Reap only the Process acquired by this startup attempt, with bounded waits."""
    if proc.returncode is None:
        try: proc.terminate()
        except ProcessLookupError: pass
    try:
        await asyncio.wait_for(proc.wait(), 3)
    except TimeoutError:
        try: proc.kill()
        except ProcessLookupError: pass
        await asyncio.wait_for(proc.wait(), 3)

async def start_gateway(arm, binary, reference, upstream_port, quota, temp, event):
    from benchmark_accounting import (
        build_gateway_config,
        validate_runtime_config,
        validate_written_config,
    )
    # Binding a temporary server lets the OS select a port; foreground readiness detects any race.
    probe = await asyncio.start_server(lambda r,w: w.close(), "127.0.0.1", 0)
    port = probe.sockets[0].getsockname()[1]
    probe.close(); await probe.wait_closed()
    product_arm = arm in ("production_rr", "production_actual")
    launched = binary if product_arm else reference
    launched_identity = {
        "path": str(launched),
        "sha256": digest(launched),
        "features": [] if product_arm else ["bench-harness"],
    }
    config, config_metadata = build_gateway_config(
        arm=arm,
        listen_port=port,
        upstream_port=upstream_port,
        quota=quota,
        launched_binary=launched_identity,
        cap=CAP,
        roots=ROOTS,
    )
    config_path = Path(temp)/"gateway.toml"
    config_path.write_text(config)
    if product_arm:
        config_metadata = validate_written_config(
            config_path,
            config_metadata,
            expected_arm=arm,
            expected_quota=quota,
            cap=CAP,
            roots=ROOTS,
        )
    paths_process = await asyncio.create_subprocess_exec(str(binary), "doctor", "--config", str(config_path), "--json", stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE)
    try:
        paths_output, _ = await asyncio.wait_for(paths_process.communicate(), timeout=10)
    finally:
        cleanup = asyncio.create_task(abort_startup(paths_process))
        while not cleanup.done():
            try: await asyncio.shield(cleanup)
            except asyncio.CancelledError: continue
        cleanup.result()
    if paths_process.returncode != 0:
        raise RuntimeError("gateway state path lookup failed")
    state = Path(json.loads(paths_output)["state_directory"]); state.mkdir(mode=0o700)
    for name in ("data", "control"):
        path = state/(name+"-token")
        path.write_text("synthetic-benchmark-"+name)
        path.chmod(0o600)
    policy = {"benchmark_fifo": "fifo", "benchmark_rr": "rr", "benchmark_backfill": "backfill"}
    args = [str(binary), "run", "--config", str(config_path)] if product_arm else [str(reference), "--config", str(config_path), "--policy", policy[arm]]
    with open(Path(temp)/"gateway.log", "wb") as log:
        proc = await asyncio.create_subprocess_exec(*args, stdout=log, stderr=log)
    try:
        started = time.monotonic()
        event({"event": "gateway_started", "pid": proc.pid})
        for _ in range(200):
            if proc.returncode is not None:
                raise RuntimeError("gateway failed startup")
            try:
                await control(port, "health")
                status = await control(port) if product_arm else None
            except (OSError, ValueError, asyncio.IncompleteReadError):
                await asyncio.sleep(.025)
                continue
            if product_arm:
                validate_runtime_config(config_metadata, status)
            config_metadata["started_monotonic_s"] = started
            return proc, port, config_metadata
        raise RuntimeError("gateway readiness deadline")
    except BaseException as error:
        # Ownership transfers to run_arm only on return. Repeated cancellation
        # must not interrupt reaping or replace the original startup exception.
        cleanup = asyncio.create_task(abort_startup(proc))
        while not cleanup.done():
            try: await asyncio.shield(cleanup)
            except asyncio.CancelledError: continue
            except Exception: break
        try: cleanup.result()
        except Exception as failure: error.add_note(f"startup cleanup failed: {failure}")
        raise

async def stop_gateway(proc, port):
    if proc is None:
        return
    if proc.returncode is None:
        try:
            await asyncio.wait_for(control(port, "stop", "POST"), 2)
            await asyncio.wait_for(proc.wait(), 12)
        except (OSError, TimeoutError, asyncio.IncompleteReadError):
            proc.terminate()
            try:
                await asyncio.wait_for(proc.wait(), 3)
            except TimeoutError:
                proc.kill(); await proc.wait()
    if proc.returncode != 0:
        raise RuntimeError(f"owned gateway exit {proc.returncode}")

async def run_arm(arm, seed, windows, binary, reference, phase, directory):
    from benchmark_http import Mock, Client, request_once
    overhead = phase == "no_wait"
    smoke = phase == "smoke"
    quota = None if overhead or smoke else QUOTA
    rows = workload(seed, 1 if smoke else windows, smoke)
    if overhead:
        rows = [dict(id=f"overhead-{i}", root=i%ROOTS, input_bytes=96, output_reservation=32,
                     actual_ratio=1, service_ms=0, timeout_s=2, offset_s=0, length="short", cost_case="matching", workload="no_wait", window=0, workflow_id=f"overhead-pair{i//2}") for i in range(100)]
    duration = 3 if smoke else windows*WINDOW
    run_id = f"{phase}-seed{seed}-{arm}"
    journal_path = directory/(run_id+".events.jsonl")
    journal = open(journal_path, "x")
    def event(value):
        journal.write(json.dumps(value, sort_keys=True)+"\n"); journal.flush()
    event({"event": "started", "id": run_id, "pid": os.getpid(), "utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())})
    mock = Mock(quota)
    mock.plans = {r["id"]: r for r in rows}
    if overhead:
        mock.plans.update({f"warmup-{i}": rows[0] for i in range(5)})
    await mock.start()
    proc = None; port = mock.port
    monitor = None
    tasks = []
    run = {"id": run_id, "phase": phase, "seed": seed, "arm": arm, "submitted": rows, "outcomes": [], "attempts": [], "resource_samples": [], "snapshots": [], "load_start": os.getloadavg(), "quota_windows": windows if quota else 0,
           "measurement": "HTTP estimated gateway cost versus fixture-defined actual cost", "client_retry": 0, "gateway_retry": 0, "cache": "off", "client_pool": "one reused connection sequential" if overhead else "new connection per ingress", "mock_quota": quota}
    event({"event": "submitted_manifest", "requests": rows})
    with tempfile.TemporaryDirectory(prefix="llmgw-bench-") as temp:
        try:
            if arm != "direct":
                proc, port, run["config"] = await start_gateway(arm, binary, reference, mock.port, quota, temp, event)
                await asyncio.sleep(1)
                run["idle_resource"] = await owned_resources(proc.pid)
                run["initial_status"] = await control(port)
            if quota:
                # No data warmup: the production 60s startup hold remains intact in every arm.
                await asyncio.sleep(WINDOW+0.1)
            if proc: run["measurement_start_resource"] = await owned_resources(proc.pid)
            start = time.monotonic()
            run["measurement_duration_s"] = None if overhead else duration
            run["start_monotonic_s"] = start
            run["started_utc"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
            mock.origin = start
            async def observe():
                while True:
                    run.setdefault("periodic_mock_budgets", []).append(mock.budget())
                    if proc:
                        resource = await owned_resources(proc.pid)
                        if resource: run["resource_samples"].append(resource)
                        status = await control(port)
                        run["snapshots"].append({"elapsed_s": time.monotonic()-start, "status": status})
                    await asyncio.sleep(1)
            if not overhead:
                monitor = asyncio.create_task(observe())
            if overhead:
                client = Client(port)
                # Record warmups separately, never admit their successes into measured samples.
                run["warmup_outcomes"] = []
                for i in range(5):
                    row = dict(rows[0], id=f"warmup-{i}")
                    run["warmup_outcomes"].append(await request_once(row, port, arm, start, client))
                if proc: run["post_warmup_status"] = await control(port)
                for row in rows:
                    result = await request_once(row, port, arm, time.monotonic(), client)
                    run["outcomes"].append(result); event({"event": "terminal", **result})
                await client.close()
            else:
                async def submit(row):
                    await asyncio.sleep(max(0, start+row["offset_s"]-time.monotonic()))
                    result = await request_once(row, port, arm, start)
                    run["outcomes"].append(result); event({"event": "terminal", **result})
                tasks = [asyncio.create_task(submit(row)) for row in rows]
                await asyncio.sleep(duration)
                run["at_measurement_end"] = {"elapsed_s": time.monotonic()-start, "terminal": len(run["outcomes"]), "pending_ids": sorted(set(r["id"] for r in rows)-set(r["id"] for r in run["outcomes"]))}
                event({"event": "measurement_end", **run["at_measurement_end"]})
                # Every request has its own 125s timeout; do not drop pending IDs at segment or run boundaries.
                await asyncio.wait_for(asyncio.gather(*tasks), 130)
            await mock.drain()
            if monitor:
                monitor.cancel()
                monitor_result = (await asyncio.gather(monitor, return_exceptions=True))[0]
                if isinstance(monitor_result, Exception): raise monitor_result
            if proc:
                run["final_status"] = await control(port)
                run["final_resource"] = await owned_resources(proc.pid)
                if run["final_status"]["admission"]["active"] or run["final_status"]["admission"]["queue_length"]:
                    # Close propagation can trail client EOF/cancellation; await bounded cleanup.
                    for _ in range(100):
                        await asyncio.sleep(.05)
                        run["final_status"] = await control(port)
                        if not run["final_status"]["admission"]["active"] and not run["final_status"]["admission"]["queue_length"]: break
                    else: raise RuntimeError("gateway retains pending work after terminal outcomes")
            run["elapsed_s"] = time.monotonic()-start
            run["attempts"] = [a for a in mock.attempts if not a["ingress_id"].startswith("warmup-")]
            run["gateway_started_attempts"] = run["final_status"]["upstream_attempts"]-len([a for a in mock.attempts if a["ingress_id"].startswith("warmup-")]) if proc else None
            run["warmup_attempts"] = [a for a in mock.attempts if a["ingress_id"].startswith("warmup-")]
            run["mock_budget_samples"] = mock.budget_samples
            run["load_end"] = os.getloadavg()
            first_attempts = {}
            for attempt in run["attempts"]:
                first_attempts.setdefault(attempt["ingress_id"], attempt)
            for outcome in run["outcomes"]:
                outcome["within_measurement"] = overhead or outcome["ended_s"] <= start+duration
                attempt = first_attempts.get(outcome["id"])
                outcome["send_to_mock_receive_ms"] = (attempt["received_s"]-outcome["sent_s"])*1000 if attempt else None
            validate_run(run)
            if arm in ("production_rr", "production_actual") and phase in ("quota", "smoke"):
                from benchmark_accounting import validate_accounting_run
                validate_accounting_run(
                    run,
                    expected_arm=arm,
                    expected_seed=seed,
                    expected_phase=phase,
                    expected_rows=rows,
                    expected_duration_s=duration,
                    expected_windows=windows if quota else 0,
                    expected_quota=quota,
                    expected_binary=run["config"]["launched_binary"],
                )
            if smoke and collections.Counter(r["outcome"] for r in run["outcomes"]) != {"completed": 17, "cancelled": 3}:
                raise ValueError("smoke payload/cancellation expectation failed")
            if overhead and any(r["outcome"] != "completed" for r in run["outcomes"]+run["warmup_outcomes"]):
                raise ValueError("no-wait fixture completion failed")
            run["summary"] = aggregate(run)
            event({"event": "validated", "submitted": len(rows), "outcomes": len(run["outcomes"]), "attempts": len(run["attempts"])})
        except BaseException as error:
            for task in tasks:
                if not task.done(): task.cancel()
            await asyncio.gather(*tasks, return_exceptions=True)
            run["attempts"] = list(mock.attempts)
            run["failure"] = type(error).__name__+": "+str(error)
            run["pending_ids_on_failure"] = sorted(set(r["id"] for r in rows)-set(r["id"] for r in run["outcomes"]))
            write_json(directory/(run_id+".failed.json"),run)
            event({"event":"failed", "error":run["failure"], "pending_ids":run["pending_ids_on_failure"]})
            raise
        finally:
            if monitor and not monitor.done():
                monitor.cancel(); await asyncio.gather(monitor, return_exceptions=True)
            await stop_gateway(proc, port)
            await mock.close()
            journal.close()
    run["owned_processes_cleaned"] = True
    write_json(directory/(run_id+".json"), run)
    return run

def identities(binary, reference, mode):
    source_paths = sorted(p for folder in ("src", "scripts", "examples", "fixtures/workloads", "tests", "docs") for p in (HERE/folder).rglob("*") if p.is_file() and "__pycache__" not in p.parts)
    source_paths += [HERE/name for name in ("Cargo.toml", "Cargo.lock", "rust-toolchain.toml")]
    sources = {str(p.relative_to(HERE)): digest(p) for p in source_paths}
    benchmark_identity = ({"path": str(reference), "sha256": digest(reference), "features": ["bench-harness"]}
                          if mode == "baseline" else {"used": False})
    return {"production": {"path": str(binary), "sha256": digest(binary), "features": []},
            "benchmark": benchmark_identity, "sources": sources}

def resume_preflight(manifest, directory, schedule, mode, windows):
    """Validate the whole discoverable state before writing or launching any arm."""
    expected = {f"{phase}-seed{seed}-{arm}": (phase, seed, arm) for phase, seed, arm in schedule}
    runs = {}
    allowed = set()
    for entry in manifest["runs"]:
        run_id = entry["id"]
        if run_id not in expected or run_id in runs:
            raise ValueError("duplicate or unexpected completed artifact ID")
        completed = directory/(run_id+".json")
        if Path(entry["path"]).resolve() != completed.resolve():
            raise ValueError("completed artifact path mismatch")
        if not completed.is_file() or digest(completed) != entry["sha256"]:
            raise ValueError(f"completed artifact hash mismatch: {run_id}")
        run = json.loads(completed.read_text())
        phase, seed, arm = expected[run_id]
        if (run.get("id"), run.get("phase"), run.get("seed"), run.get("arm")) != (run_id, phase, seed, arm):
            raise ValueError(f"completed artifact content identity mismatch: {run_id}")
        validate_run(run)
        if mode == "accounting":
            from benchmark_accounting import validate_accounting_run
            smoke = phase == "smoke"
            quota = None if smoke else QUOTA
            expected_rows = workload(seed, 1 if smoke else windows, smoke)
            validate_accounting_run(
                run,
                expected_arm=arm,
                expected_seed=seed,
                expected_phase=phase,
                expected_rows=expected_rows,
                expected_duration_s=3 if smoke else windows * WINDOW,
                expected_windows=0 if smoke else windows,
                expected_quota=quota,
                expected_binary=manifest["identity"]["production"],
            )
        if entry.get("summary") != run.get("summary"):
            raise ValueError("completed artifact summary mismatch")
        if mode == "accounting":
            expected_provenance = {
                "accounting": run["config"]["accounting"],
                "raw_toml_sha256": run["config"]["raw_toml_sha256"],
                "launched_binary": run["config"]["launched_binary"],
            }
            if entry.get("provenance") != expected_provenance:
                raise ValueError("completed artifact provenance mismatch")
        runs[run_id] = run
        allowed.update((completed.name, run_id+".events.jsonl"))
    for path in sorted(directory.iterdir()):
        if path.name not in allowed:
            raise ValueError(f"unregistered or incomplete artifact {path.name}: preserve and inspect its owned process/journal; use an exclusive new output, never auto-adopt or duplicate")
    if mode == "accounting":
        from benchmark_accounting import validate_accounting_pair
        for seed in manifest["seeds"]:
            reserved = runs.get(f"{schedule[0][0]}-seed{seed}-production_rr")
            actual = runs.get(f"{schedule[0][0]}-seed{seed}-production_actual")
            if reserved is not None and actual is not None:
                validate_accounting_pair(reserved, actual)
    return runs

async def main(args):
    if args.mode == "accounting":
        if not (args.smoke or args.phase == "quota"):
            raise ValueError("accounting mode requires quota phase or smoke")
        if args.pilot_only and (args.smoke or args.phase != "quota"):
            raise ValueError("pilot-only requires accounting quota mode")
        if args.windows != 5:
            raise ValueError("accounting mode requires the fixed five-window contract")
    elif args.pilot_only:
        raise ValueError("pilot-only requires accounting quota mode")
    binary = args.binary.resolve()
    reference = None if args.mode == "accounting" else args.reference_binary.resolve()
    seeds = [int(s) for s in args.seeds.split(",")]
    if not seeds or len(seeds)!=len(set(seeds)) or args.windows < 1:
        raise ValueError("unique seeds and positive windows required")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    directory = args.output.parent/(args.output.stem+"-runs")
    directory.mkdir(exist_ok=True)
    identity = identities(binary, reference, args.mode)
    phases = ["smoke"] if args.smoke else ["no_wait", "quota"] if args.phase=="all" else [args.phase]
    if args.mode == "accounting":
        from benchmark_accounting import paired_schedule
        schedule = paired_schedule(seeds, phases[0])
    else:
        schedule = [(phase, seed, arm) for phase in phases for index, seed in enumerate(seeds) for arm in (ARMS[index%4:]+ARMS[:index%4])]
    manifest = {"schema": 1, "status": "in_progress", "identity": identity, "seeds": seeds, "windows": args.windows,
                "mode": args.mode, "schedule": schedule, "host": {"platform": platform.platform(), "machine": platform.machine(), "python": sys.version.split()[0], "cpus": os.cpu_count(), "clock": vars(time.get_clock_info("monotonic"))}, "runs": []}
    if args.output.exists():
        old = json.loads(args.output.read_text())
        if old["mode"] != args.mode or old["identity"] != identity or old["seeds"] != seeds or old["windows"] != args.windows or old["schedule"] != [list(row) for row in schedule]:
            raise ValueError("resume identity differs; use an exclusive new output path")
        manifest = old
    resumed = resume_preflight(manifest, directory, schedule, args.mode, args.windows)
    if platform.system()=="Darwin":
        proc=await asyncio.create_subprocess_exec("sysctl","-n","hw.memsize","machdep.cpu.brand_string",stdout=asyncio.subprocess.PIPE)
        output,_=await proc.communicate()
        fields=output.decode().splitlines()
        if proc.returncode==0 and len(fields)==2:
            manifest["host"].update(ram_bytes=int(fields[0]),cpu_model=fields[1])
    write_json(args.output, manifest)
    execution_schedule = schedule[:2] if args.mode == "accounting" and args.pilot_only else schedule
    for phase, seed, arm in execution_schedule:
        run_id = f"{phase}-seed{seed}-{arm}"
        completed = directory/(run_id+".json")
        if run_id in resumed:
            run = resumed[run_id]
        else:
            if completed.exists() or (directory/(run_id+".events.jsonl")).exists() or (directory/(run_id+".failed.json")).exists():
                raise ValueError(f"incomplete run {run_id}: inspect its owned process/journal; never duplicate a live run")
            print(f"START {run_id}", flush=True)
            run = await run_arm(arm, seed, args.windows, binary, reference, phase, directory)
            print(f"END {run_id} {json.dumps(run['summary']['all'])}", flush=True)
        if args.mode == "accounting":
            from benchmark_accounting import validate_accounting_run
            smoke = phase == "smoke"
            validate_accounting_run(
                run,
                expected_arm=arm,
                expected_seed=seed,
                expected_phase=phase,
                expected_rows=workload(seed, 1 if smoke else args.windows, smoke),
                expected_duration_s=3 if smoke else args.windows * WINDOW,
                expected_windows=0 if smoke else args.windows,
                expected_quota=None if smoke else QUOTA,
                expected_binary=identity["production"],
            )
        if not any(r["id"]==run_id for r in manifest["runs"]):
            entry = {"id": run_id, "path": str(completed), "sha256": digest(completed), "summary": run["summary"]}
            if args.mode == "accounting":
                entry["provenance"] = {
                    "accounting": run["config"]["accounting"],
                    "raw_toml_sha256": run["config"]["raw_toml_sha256"],
                    "launched_binary": run["config"]["launched_binary"],
                }
            manifest["runs"].append(entry)
        write_json(args.output, manifest)
    if identities(binary, reference, args.mode) != identity:
        raise ValueError("source/binary identity changed during run")
    if args.mode == "accounting":
        from benchmark_accounting import paired_summary
        registered = {entry["id"] for entry in manifest["runs"]}
        completed_pairs = sum(
            all(f"{phases[0]}-seed{seed}-{arm}" in registered for arm in ("production_rr", "production_actual"))
            for seed in seeds
        )
        manifest["planned_pairs"] = len(seeds)
        manifest["completed_pairs"] = completed_pairs
        manifest["matrix_complete"] = len(manifest["runs"]) == len(schedule)
        runs = [json.loads(Path(entry["path"]).read_text()) for entry in manifest["runs"]]
        manifest["evaluation"] = paired_summary(runs, seeds, phases[0])
        if args.pilot_only:
            if completed_pairs != 1:
                raise ValueError("pilot checkpoint requires one complete pair")
            manifest["matrix_complete"] = False
            manifest["status"] = "accounting_pilot_completed"
        elif args.smoke:
            manifest["status"] = "completed_smoke"
        elif manifest["matrix_complete"]:
            manifest["status"] = "completed_accounting_matrix"
        else:
            raise ValueError("accounting matrix ended incomplete")
    else:
        manifest["evaluation"] = evaluate(manifest)
        manifest["status"] = "completed_smoke" if args.smoke else ("completed_composite_matrix" if args.phase=="all" else "completed_"+args.phase+"_only")
    write_json(args.output, manifest)
    print(manifest["status"], flush=True)

def evaluate(manifest):
    runs = [json.loads(Path(entry["path"]).read_text()) for entry in manifest["runs"]]
    result = {"budgets_tentative": {"paired_proxy_latency_difference_p95_ms": 2, "idle_rss_mib": 50}, "comparisons": [], "quota_arms": {}}
    for arm in ARMS[1:]:
        seed_results = []
        for seed in manifest["seeds"]:
            direct = next((r for r in runs if r["phase"]=="no_wait" and r["seed"]==seed and r["arm"]=="direct"), None)
            proxy = next((r for r in runs if r["phase"]=="no_wait" and r["seed"]==seed and r["arm"]==arm), None)
            if direct is None or proxy is None: continue
            baseline = {r["id"]:r for r in direct["outcomes"]}
            pairs = [{"id":r["id"], "difference_ms":r["elapsed_ms"]-baseline[r["id"]]["elapsed_ms"]} for r in proxy["outcomes"]]
            seed_results.append({"seed":seed, "paired_latency_difference_ms":distribution([r["difference_ms"] for r in pairs]),
                "pairs":pairs, "marginal_p95_difference_ms":proxy["summary"]["all"]["completion_ms_success_only"]["p95"]-direct["summary"]["all"]["completion_ms_success_only"]["p95"],
                "idle_rss_mib":proxy["idle_resource"]["rss_bytes"]/(1024**2)})
        values = [r["paired_latency_difference_ms"]["p95"] for r in seed_results]
        idle = [r["idle_rss_mib"] for r in seed_results]
        result["comparisons"].append({"arm":arm, "seeds":seed_results,
            "pairing":"same seed and request index from separate sequential runs; negative differences retained; not internal CPU overhead",
            "p95_across_seed_range_ms": [min(values),max(values)] if values else None,
            "tentative_latency_budget_observation": "insufficient" if len(values)<5 else ("all_seeds_below_2ms" if max(values)<2 else "crosses_2ms_inconclusive" if min(values)<2 else "all_seeds_at_or_above_2ms"),
            "tentative_idle_rss_budget_observation": "insufficient" if len(idle)<5 else "all_samples_below_50MiB" if max(idle)<50 else "sample_exceeds_50MiB",
            "uncertainty":"empirical seed range, not confidence interval; differences smaller than this variability do not establish superiority"})
    for arm in ARMS:
        quota_runs=[r for r in runs if r["phase"]=="quota" and r["arm"]==arm]
        if not quota_runs: continue
        joined={"submitted":[], "outcomes":[], "attempts":[]}
        for run in quota_runs:
            prefix=run["id"]+":"
            joined["submitted"] += [dict(r,id=prefix+r["id"],workflow_id=prefix+r["workflow_id"]) for r in run["submitted"]]
            joined["outcomes"] += [dict(r,id=prefix+r["id"]) for r in run["outcomes"]]
            joined["attempts"] += [dict(r,id=prefix+r["id"],ingress_id=prefix+r["ingress_id"]) for r in run["attempts"]]
        joined["gateway_started_attempts"] = sum(r["gateway_started_attempts"] for r in quota_runs) if arm != "direct" else None
        validate_run(joined)
        summary=aggregate(joined)
        resource=[s["rss_bytes"] for r in quota_runs for s in r["resource_samples"]]
        summary["sampled_peak_gateway_rss_mib"] = max(resource)/(1024**2) if resource else None
        summary["gateway_cpu_seconds_by_seed"] = [{"seed":r["seed"],"cpu_seconds":r["final_resource"]["cpu_seconds"]-r["measurement_start_resource"]["cpu_seconds"]} for r in quota_runs if r.get("final_resource")]
        budgets=[s["tpm_unused_fixture_units"] for r in quota_runs for s in r.get("periodic_mock_budgets",[]) if s["elapsed_s"] <= r["measurement_duration_s"]]
        summary["sampled_unused_mock_tpm_fixture_units"] = distribution(budgets)
        summary["upstream_attempt_outcomes"] = dict(collections.Counter(a["outcome"] for a in joined["attempts"]))
        cancelled=set(r["id"] for r in joined["outcomes"] if r["outcome"]=="cancelled")
        summary["attempt_outcomes_after_cancelled_ingress"] = dict(collections.Counter(a["outcome"] for a in joined["attempts"] if a["ingress_id"] in cancelled))
        result["quota_arms"][arm]=summary
    return result

async def http_self_check():
    from benchmark_http import Mock, request_once
    mock=Mock({"rpm":2,"tpm":10000})
    rows=workload(1,1)[:3]
    mock.plans={r["id"]:r for r in rows}
    await mock.start()
    try:
        outcomes=[await request_once(r,mock.port,"direct",time.monotonic()) for r in rows]
        assert [r["outcome"] for r in outcomes]==["completed","completed","rejected"]
        assert len(mock.attempts)==3 and all(a["estimated_cost"]==a["actual_cost_fixture_units"] for a in mock.attempts)
        assert mock.budget()["rpm_used"]==2
        mock.debits=collections.deque((t-60,c) for t,c in mock.debits)
        assert mock.budget()["rpm_used"]==0
        print("PASS actual HTTP quota counts, matching costs, rolling expiration")
    finally:
        await mock.close()
    async def truncated(reader,writer):
        await reader.readuntil(b"\r\n\r\n")
        data=b'data: {"choices":[{"delta":{"content":"fixture:test"}}]}\n\n'
        writer.write(f"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {len(data)}\r\n\r\n".encode()+data)
        await writer.drain(); writer.close(); await writer.wait_closed()
    server=await asyncio.start_server(truncated,"127.0.0.1",0)
    try:
        result=await request_once(dict(rows[0],id="test"),server.sockets[0].getsockname()[1],"direct",time.monotonic())
        assert result["outcome"]=="error" and result["status"]==200
        print("REJECTED HTTP200 SSE without terminal marker")
    finally:
        server.close(); await server.wait_closed()

async def boundary_self_check():
    from benchmark_http import Client
    async def bad_trailer(reader, writer):
        await reader.readuntil(b"\r\n\r\n")
        writer.write(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n")
        await writer.drain(); writer.close(); await writer.wait_closed()
    server=await asyncio.start_server(bad_trailer,"127.0.0.1",0)
    client=Client(server.sockets[0].getsockname()[1])
    try:
        try: await asyncio.wait_for(client.exchange("GET","/",b"",{}),.5)
        except asyncio.IncompleteReadError: print("REJECTED truncated chunk trailer EOF without blocking loop")
        else: raise AssertionError("accepted truncated trailer")
    finally:
        await client.close(); server.close(); await server.wait_closed()
    rows=workload(1,1)[:2]
    run={"submitted":rows, "outcomes":[dict(id=r["id"],outcome="completed",payload_valid=True,elapsed_ms=1,scheduling_lag_ms=0,within_measurement=(i==0)) for i,r in enumerate(rows)],"attempts":[]}
    summary=aggregate(run)
    assert summary["all"]["outcomes"]=={"completed":2}
    assert summary["all"]["within_measurement_completed"]==1
    assert summary["all"]["post_measurement_drain_outcomes"]=={"completed":1}
    assert summary["synthetic_pair_workflows"]["completed_within_measurement"]==0
    print("PASS post-window drain successes excluded from fixed-time goodput")

async def resume_self_check():
    """Exercise real main resume logic using synthetic files and forbid any new arm."""
    global ARMS, run_arm
    previous_arms, previous_runner = ARMS, run_arm
    async def forbidden_run(*args, **kwargs):
        raise AssertionError("resume self-check attempted to launch an arm")
    ARMS, run_arm = ("direct",), forbidden_run
    try:
        with tempfile.TemporaryDirectory(prefix="llmgw-resume-check-") as tmp:
            root=Path(tmp)
            binary=root/"not-executable"; binary.write_bytes(b"synthetic identity only"); binary=binary.resolve()
            output=root/"resume.json"; directory=root/"resume-runs"; directory.mkdir()
            run_id="no_wait-seed1-direct"; completed=directory/(run_id+".json")
            synthetic={"id":run_id,"phase":"no_wait","arm":"direct","seed":1,"submitted":[],"outcomes":[],"attempts":[],"summary":{}}
            write_json(completed,synthetic)
            args=argparse.Namespace(binary=binary,reference_binary=binary,seeds="1",windows=1,output=output,smoke=False,phase="no_wait",mode="baseline",pilot_only=False)
            manifest={"schema":1,"status":"in_progress","mode":"baseline","identity":identities(binary,binary,"baseline"),"seeds":[1],"windows":1,"schedule":[["no_wait",1,"direct"]],"host":{},"runs":[{"id":run_id,"path":str(completed),"sha256":digest(completed),"summary":{}}]}
            write_json(output,manifest)
            await main(args)
            assert len(json.loads(output.read_text())["runs"])==1
            print("PASS identical completed artifact resumes without launching an arm")
            recorded=json.loads(output.read_text())
            alternate=root/"redirected.json"; write_json(alternate,synthetic)
            redirected=copy.deepcopy(recorded); redirected["runs"][0]["path"]=str(alternate)
            write_json(output,redirected)
            try: await main(args)
            except ValueError as error:
                assert "artifact path" in str(error)
                print("REJECTED redirected completed artifact path")
            else: raise AssertionError("resume accepted redirected artifact path")
            duplicate=copy.deepcopy(recorded); duplicate["runs"].append(copy.deepcopy(duplicate["runs"][0]))
            write_json(output,duplicate)
            try: await main(args)
            except ValueError as error:
                assert "artifact ID" in str(error)
                print("REJECTED duplicate completed artifact ID")
            else: raise AssertionError("resume accepted duplicate artifact ID")
            write_json(output,recorded)
            write_json(completed,dict(synthetic,modified_after_recording=True))
            try: await main(args)
            except ValueError as error:
                assert "artifact" in str(error) and "hash" in str(error)
                print("REJECTED modified completed artifact on real resume path")
            else: raise AssertionError("resume accepted modified completed artifact with stale manifest SHA")
            completed.unlink()
            try: await main(args)
            except ValueError as error:
                assert "artifact" in str(error)
                print("REJECTED missing previously recorded completed artifact")
            else: raise AssertionError("resume accepted missing recorded artifact")
            for field, wrong in (("id", "no_wait-seed999-direct"), ("phase", "quota"), ("seed", 999), ("arm", "production_rr")):
                write_json(completed, dict(synthetic, **{field: wrong}))
                malformed=copy.deepcopy(recorded); malformed["runs"][0]["sha256"]=digest(completed)
                write_json(output,malformed)
                before=output.read_bytes(),completed.read_bytes()
                try: await main(args)
                except ValueError as error: assert "content identity" in str(error)
                else: raise AssertionError(f"resume accepted mismatched {field}")
                assert before == (output.read_bytes(),completed.read_bytes())
            print("REJECTED registered ID/phase/seed/arm mismatch even with matching SHA")
            output.unlink()
            write_json(completed,synthetic)
            try: await main(args)
            except ValueError as error: assert "unregistered" in str(error)
            else: raise AssertionError("fresh output adopted orphan")
            assert not output.exists()
            print("REJECTED orphan on fresh output before writing manifest or launching")
            completed.unlink()
            args.seeds="998,999"
            later=directory/"no_wait-seed999-direct.json"; write_json(later,synthetic)
            pending=copy.deepcopy(manifest); pending.update(seeds=[998,999],schedule=[["no_wait",998,"direct"],["no_wait",999,"direct"]],runs=[])
            write_json(output,pending)
            before=output.read_bytes(),later.read_bytes()
            try: await main(args)
            except ValueError as error: assert "unregistered" in str(error)
            else: raise AssertionError("later orphan accepted")
            assert before == (output.read_bytes(),later.read_bytes())
            print("REJECTED later orphan before any earlier missing arm launches")
            later.unlink()
            journal=directory/"no_wait-seed999-direct.events.jsonl"; journal.write_text("owned interrupted fixture\n")
            try: await main(args)
            except ValueError as error: assert "incomplete" in str(error)
            else: raise AssertionError("later incomplete journal accepted")
            print("REJECTED later incomplete journal before any new arm")
    finally:
        ARMS, run_arm = previous_arms, previous_runner

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-check", action="store_true")
    parser.add_argument("--phase", choices=["all","no_wait","quota"], default="all")
    parser.add_argument("--mode", choices=["baseline", "accounting"], default="baseline")
    parser.add_argument("--pilot-only", action="store_true")
    parser.add_argument("--smoke", action="store_true", help="short unlimited-quota fixture validation, not a quota matrix")
    parser.add_argument("--binary", type=Path, default=Path("target/release/llmgw"))
    parser.add_argument("--reference-binary", type=Path, default=Path("target/bench/release/examples/bench_gateway"))
    parser.add_argument("--seeds", default="1,2,3,4,5")
    parser.add_argument("--windows", type=int, default=5)
    parser.add_argument("--output", type=Path, default=Path("artifacts/pilot.json"))
    args = parser.parse_args()
    if args.self_check:
        self_check()
        asyncio.run(http_self_check())
        asyncio.run(boundary_self_check())
        asyncio.run(resume_self_check())
        print("self-check PASS")
    else: asyncio.run(main(args))
