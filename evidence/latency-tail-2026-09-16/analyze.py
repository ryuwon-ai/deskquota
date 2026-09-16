"""Recalculate saved timings and reconstruct quota gates; no gateway/API execution."""
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "product/scripts"))
from benchmark import distribution

BASE = ROOT / "evidence/current-competitor-comparison-2026-09-15"
ARMS = ["deskquota_native_actual", "bifrost", "litellm", "hivemind", "direct"]
data, outcomes, paths = {}, {}, {}
for arm in ARMS:
    path = BASE / f"quota-quarter_actual-{arm}/result.json"
    paths[str(path.relative_to(ROOT))] = hashlib.sha256(path.read_bytes()).hexdigest()
    data[arm] = json.loads(path.read_text())
    d = data[arm]
    outcomes[arm] = {r["id"]: r for r in d["outcomes"]}
    assert len(outcomes[arm]) == len(d["outcomes"]) == len(d["submitted"]) == 18
    assert d["submitted"] == data[ARMS[0]]["submitted"]
    assert d["quota"] == {"rpm": 16, "tpm": 6000} and d["cap"] == 2
    assert d["client_retries"] == d["gateway_retries"] == 0 and d["cache"] == "off"
    assert not d["protocol_errors"]

d = data[ARMS[0]]
rows = outcomes[ARMS[0]]
plans = {r["id"]: r for r in d["submitted"]}
attempts = {a["ingress_id"]: a for a in d["attempts"]}
assert len(attempts) == len(d["attempts"]) == 18
assert all(r["outcome"] == "completed" for r in rows.values())
origin = d["measurement_start_s"]
starts = sorted(attempts.values(), key=lambda a: a["received_s"])
assert [a["ingress_id"] for a in starts[-2:]] == ["g0", "g16"]
assert sum(r["within_measurement"] for r in rows.values()) == 16
assert plans["g0"]["root"] == plans["g16"]["root"] == 0

def costs(i):
    a, p = attempts[i], plans[i]
    return a["body_bytes"] + p["output_reservation"], a["actual_cost_fixture_units"]

def projection(at):
    # Observation boundaries approximate internal start/finish within transport latency.
    # This is arithmetic reconstruction, not an executed production-ledger trace.
    live = [i for i, a in attempts.items() if a["received_s"] <= at < a["received_s"] + 60]
    active = [i for i in live if rows[i]["ended_s"] > at]
    used = sum(costs(i)[0 if i in active else 1] for i in live)
    return {"elapsed_s": at - origin, "rpm_used": len(live), "tpm_used": used,
            "tpm_free": 6000 - used, "active": active,
            "g0_fits": len(live) < 16 and len(active) < 2 and used + costs("g0")[0] <= 6000,
            "g16_fits": len(live) < 16 and len(active) < 2 and used + costs("g16")[0] <= 6000}

expiry1, expiry2 = [a["received_s"] + 60 for a in starts[:2]]
stages = {"both_waiting": projection(rows["g16"]["sent_s"] + .001),
          "first_rpm_expiry": projection(expiry1 + .00001),
          "second_rpm_expiry": projection(expiry2 + .00001),
          "g0_running": projection(attempts["g0"]["received_s"] + .001),
          "g0_finished": projection(rows["g0"]["ended_s"] + .00001)}
assert (stages["both_waiting"]["rpm_used"], stages["both_waiting"]["tpm_used"]) == (16, 4297)
assert stages["both_waiting"]["active"] == []
assert stages["first_rpm_expiry"]["tpm_free"] == 1815
assert not stages["first_rpm_expiry"]["g0_fits"] and stages["first_rpm_expiry"]["g16_fits"]
assert stages["second_rpm_expiry"]["tpm_free"] == 2103
assert stages["second_rpm_expiry"]["g0_fits"]
assert stages["g0_running"]["tpm_free"] == 46 and not stages["g0_running"]["g16_fits"]
assert stages["g0_finished"]["tpm_free"] == 1700 and stages["g0_finished"]["g16_fits"]

timings, peers = {}, {}
for i in ("g0", "g16", "g11"):
    a, r = attempts[i], rows[i]
    timings[i] = {"sent_s": r["sent_s"] - origin, "upstream_s": a["received_s"] - origin,
                  "root": plans[i]["root"],
                  "completed_s": r["ended_s"] - origin,
                  "before_upstream_s": a["received_s"] - r["sent_s"],
                  "after_upstream_s": r["ended_s"] - a["received_s"],
                  "total_s": r["elapsed_ms"] / 1000, "reservation_and_actual": costs(i)}
    assert abs(timings[i]["before_upstream_s"] + timings[i]["after_upstream_s"] - timings[i]["total_s"]) < .00001
for arm in ARMS[1:]:
    common = sorted(i for i, r in outcomes[arm].items() if r["outcome"] == "completed")
    rejected = {i: {"status": r["status"], "elapsed_s": r["elapsed_ms"] / 1000,
                    "upstream_attempts": sum(a["ingress_id"] == i for a in data[arm]["attempts"])}
                for i, r in outcomes[arm].items() if r["outcome"] == "rejected"}
    peers[arm] = {"common_success_ids": common, "rejected": rejected,
                  "deskquota_common_ms": distribution([rows[i]["elapsed_ms"] for i in common]),
                  "peer_common_ms": distribution([outcomes[arm][i]["elapsed_ms"] for i in common])}
assert set(peers["bifrost"]["rejected"]) == {"g0", "g16"}
assert all(r["status"] == 429 and r["upstream_attempts"] == 0
           for arm in ("bifrost", "litellm") for r in peers[arm]["rejected"].values())

# Idealized tail-only counterfactual, NOT a new benchmark or an available estimator.
# Keep first 16 observed starts/usage and FIFO. Substitute fixture input units,
# keeping the original output caps; no foreknowledge of actual output is needed.
tail_reservations = {i: plans[i]["canonical_input_units"] + plans[i]["output_reservation"]
                     for i in ("g0", "g16")}
assert tail_reservations == {"g0": 563, "g16": 304}
assert tail_reservations["g0"] <= stages["first_rpm_expiry"]["tpm_free"]
assert sum(tail_reservations.values()) <= stages["second_rpm_expiry"]["tpm_free"]
ideal = {i: expiry + plans[i]["service_ms"] / 1000 - rows[i]["sent_s"]
         for i, expiry in (("g0", expiry1), ("g16", expiry2))}
assert expiry1 + plans["g0"]["service_ms"] / 1000 < expiry2
observed_p95 = distribution([r["elapsed_ms"] / 1000 for r in rows.values()])["p95"]
ideal_p95 = distribution([ideal.get(i, r["elapsed_ms"] / 1000) for i, r in rows.items()])["p95"]
assert observed_p95 == timings["g16"]["total_s"] and 38 < ideal_p95 < 39

print(json.dumps({"scope": "Saved HTTP timestamps + source-supported quota reconstruction; no new runtime benchmark",
                  "input_sha256": paths, "timings": timings, "ledger_projection": stages, "peers": peers,
                  "rpm_expiries_s": [expiry1-origin, expiry2-origin],
                  "g0_extra_wait_after_first_rpm_s": attempts["g0"]["received_s"]-expiry1,
                  "idealized_tail_costs": {"assumption": "fixture input units + unchanged output caps; first 16 starts held fixed; no transport overhead",
                                          "reservations": tail_reservations, "latencies_s": ideal, "p95_s": ideal_p95,
                                          "observed_p95_s": observed_p95,
                                          "difference_s": observed_p95-ideal_p95},
                  "checks": "PASS"}, indent=2) + "\n")
