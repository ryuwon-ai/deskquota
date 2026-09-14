"""Read-only recheck of this round; no relaxed production-accounting validator."""
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import benchmark
from benchmark_accounting import build_gateway_config


def audit(directory):
    manifest = json.loads((directory / "manifest.json").read_text())
    assert manifest["status"].startswith("completed")
    results = []
    for item in manifest["runs"]:
        path = directory / item["path"]
        assert hashlib.sha256(path.read_bytes()).hexdigest() == item["sha256"]
        run = json.loads(path.read_text())
        benchmark.validate_run(run)
        assert run["summary"] == benchmark.aggregate(run)
        config = run["config"]
        _, expected = build_gateway_config(
            arm=run["arm"], listen_port=config["listen_port"], upstream_port=config["upstream_port"],
            quota={"rpm": 16, "tpm": 6000}, cap=manifest["cap"], roots=4,
            launched_binary={"path": manifest["identities"]["benchmark_binary"]["path"],
                             "sha256": manifest["identities"]["benchmark_binary"]["sha256"],
                             "features": ["bench-harness"]})
        assert {k: config[k] for k in expected} == expected
        for status in (run["initial_status"], run["final_status"]):
            admission = status["admission"]
            assert admission["accounting"] == "reserved"
            assert (admission["rpm_mode"], admission["rpm_capacity"]) == ("known", "16")
            assert (admission["tpm_mode"], admission["tpm_capacity"]) == ("known", "6000")
            assert [r["id"] for r in admission["roots"]] == ["r0", "r1", "r2", "r3"]
        assert run["initial_status"]["identity"] is None  # Feature example has no lifecycle identity.
        assert run["final_status"]["admission"]["active"] == 0
        assert run["final_status"]["admission"]["queue_length"] == 0
        assert run["owned_processes_cleaned"] is True
        assert len(run["attempts"]) == run["gateway_started_attempts"]
        assert len({r["ingress_id"] for r in run["attempts"]}) == len(run["attempts"])
        results.append({"id": run["id"], "terminal_outcomes": len(run["outcomes"]),
                        "upstream_attempts": len(run["attempts"]),
                        "fixed_completed": run["summary"]["all"]["within_measurement_completed"],
                        "outcomes": run["summary"]["all"]["outcomes"],
                        "recorded_config_reconstruction": "pass",
                        "observed_quota_accounting_roots": "pass",
                        "lifecycle_config_fingerprint": "unavailable_in_benchmark_example"})
    return {"directory": directory.name, "passed": True, "runs": results,
            "limitation": "Generated TOML/SHA and observed quota verified separately; no full runtime config attestation. No default-promotion evidence."}


if __name__ == "__main__":
    print(json.dumps(audit(Path(sys.argv[1])), indent=2))
