"""Recheck saved outcomes and the RPM lower bound; no HTTP or product changes.

Also execute one isolated function from the measured LiteLLM installation.
The limiter examples are arithmetic controls, not gateway benchmarks.
"""
import ast
import collections
import hashlib
import json
import math
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "product/scripts"))
from benchmark import distribution

MEASURED = ROOT / "evidence/current-competitor-comparison-2026-09-15"
CANDIDATE = ROOT / "evidence/input-estimation-2026-09-16/calibrated-original18-candidate.json"
LITELLM = Path("/Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/python/litellm-venv/lib/python3.12/site-packages/litellm/router_strategy/lowest_tpm_rpm_v2.py")


def read(path, hashes):
    data = path.read_bytes()
    hashes[str(path)] = hashlib.sha256(data).hexdigest()
    return json.loads(data)


def lower_bound(rows, arrivals, rpm):
    # With N starts and at most R per rolling 60s, b_N >= b_(N-R) + 60.
    # b_(N-R) >= a_(N-R). Last-starting request i needs s_i more seconds.
    # Therefore max latency >= a_(N-R) + 60 - max_i(a_i - s_i).
    n = len(rows)
    assert n > rpm and math.ceil(n * .95) == n
    anchor = sorted(arrivals.values())[n - rpm - 1]
    slack = max(arrivals[r["id"]] - r["service_ms"] / 1000 for r in rows)
    return anchor + 60 - slack


def limiter_controls():
    arrivals = [0.] * 16 + [30., 30.]
    rolling, active = [], collections.deque()
    tokens, last, bucket = 16., 0., []
    for arrival in arrivals:
        now = max(arrival, rolling[-1] if rolling else 0)
        while active and active[0] <= now - 60:
            active.popleft()
        if len(active) == 16:
            now = active[0] + 60
            while active and active[0] <= now - 60:
                active.popleft()
        active.append(now)
        rolling.append(now)
        at = max(arrival, last)
        tokens = min(16., tokens + (at - last) * 16 / 60)
        if tokens < 1:
            at += (1 - tokens) * 60 / 16
            tokens = 1.
        tokens -= 1
        last = at
        bucket.append(at)
    assert rolling[-2:] == [60., 60.] and bucket[-2:] == [30., 30.]
    return {"scope": "RPM-only arithmetic; no service, TPM or HTTP; bucket capacity16, refill16/minute",
            "arrivals_s": arrivals, "rolling_starts_s": rolling, "bucket_starts_s": bucket}


def litellm_control(hashes):
    data = LITELLM.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    assert digest == "04a9112ba4bd83f35b47bee25ca3d8ba52b71d3e5752fd428bb404661e23257d"
    hashes[str(LITELLM)] = digest
    tree = ast.parse(data)
    fn = next(node for node in ast.walk(tree) if isinstance(node, ast.FunctionDef)
              and node.name == "_return_potential_deployments")
    namespace = {}
    exec(compile(ast.Module(body=[fn], type_ignores=[]), str(LITELLM), "exec"), namespace)
    select = namespace[fn.name]
    deployment = {"model_info": {"id": "fixture"}, "litellm_params": {"rpm": 16, "tpm": 6000}}
    result = {str(used): len(select(None, [deployment], {"fixture": 100}, 10, {"fixture": used}))
              for used in (14, 15, 16)}
    assert result == {"14": 1, "15": 0, "16": 0}
    # This selector has no output-cap argument: 5990 observed + 10 input fits.
    assert len(select(None, [deployment], {"fixture": 5990}, 10, {"fixture": 0})) == 1
    return {"scope": "Exact isolated installed selector; not a whole-gateway test",
            "available_deployments_by_recorded_rpm": result,
            "observed5990_plus_input10_at_tpm6000": "admitted by selector"}


def main():
    hashes = {}
    current = read(CANDIDATE, hashes)
    rows = current["submitted"]
    outcomes = {o["id"]: o for o in current["outcomes"]}
    assert len(rows) == len(outcomes) == 18
    assert all(o["outcome"] == "completed" for o in outcomes.values())
    t0 = current["measurement_start_s"]
    planned = lower_bound(rows, {r["id"]: r["offset_s"] for r in rows}, 16)
    observed = lower_bound(rows, {i: o["sent_s"] - t0 for i, o in outcomes.items()}, 16)
    p95 = distribution([o["elapsed_ms"] for o in outcomes.values()])["p95"] / 1000
    assert planned <= p95 and observed <= p95 < observed + .03
    starts = sorted(a["received_s"] for a in current["attempts"])
    assert starts[-1] >= starts[1] + 60
    tails = {}
    for attempt in current["attempts"]:
        i = attempt["ingress_id"]
        if i not in ("g0", "g16", "g11"):
            continue
        o = outcomes[i]
        tails[i] = {"submit_s": o["sent_s"] - t0,
                    "upstream_received_s": attempt["received_s"] - t0,
                    "completed_s": o["ended_s"] - t0,
                    "pre_upstream_s": attempt["received_s"] - o["sent_s"],
                    "post_upstream_s": o["ended_s"] - attempt["received_s"]}
    peers = {}
    compare_fields = ("id", "root", "input_bytes", "output_reservation", "service_ms", "offset_s")
    signatures = {tuple(r[k] for k in compare_fields) for r in rows}
    for arm in ("deskquota_native_actual", "bifrost", "litellm", "hivemind", "direct"):
        x = read(MEASURED / f"quota-quarter_actual-{arm}/result.json", hashes)
        assert {tuple(r[k] for k in compare_fields) for r in x["submitted"]} == signatures
        assert x["quota"] == {"rpm": 16, "tpm": 6000} and x["cap"] == 2
        assert len(x["outcomes"]) == len({o["id"] for o in x["outcomes"]}) == 18
        success = {o["id"]: o for o in x["outcomes"] if o["outcome"] == "completed"}
        common = set(success) & set(outcomes)
        peers[arm] = {"completed": len(success),
                      "terminals": dict(collections.Counter(o["outcome"] for o in x["outcomes"])),
                      "success_p95_ms": distribution([o["elapsed_ms"] for o in success.values()])["p95"],
                      "common_ids": sorted(common),
                      "current_p95_on_same_ids_ms": distribution([outcomes[i]["elapsed_ms"] for i in common])["p95"],
                      "failures": [{"id": o["id"], "status": o.get("status"), "elapsed_ms": o["elapsed_ms"],
                                    "upstream_attempts": sum(a["ingress_id"] == o["id"] for a in x["attempts"])}
                                   for o in x["outcomes"] if o["outcome"] != "completed"]}
    result = {"scope": "Saved-record diagnosis, math and isolated selector; no fresh HTTP benchmark or real API",
              "input_sha256": hashes, "planned_rpm_lower_bound_s": planned,
              "actual_ingress_rpm_lower_bound_s": observed, "current_p95_s": p95,
              "gap_above_actual_ingress_lower_bound_ms": (p95 - observed) * 1000,
              "tails": tails, "peers": peers, "limiter_arithmetic": limiter_controls(),
              "litellm_selector": litellm_control(hashes)}
    out = ROOT / "evidence/quota-policy-deep-dive-2026-09-16/result.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"checks": "pass", "rpm_lower_bound_s": observed, "current_p95_s": p95,
                      "gap_ms": result["gap_above_actual_ingress_lower_bound_ms"],
                      "litellm_selector": result["litellm_selector"], "output": str(out)}, indent=2))


if __name__ == "__main__":
    main()
