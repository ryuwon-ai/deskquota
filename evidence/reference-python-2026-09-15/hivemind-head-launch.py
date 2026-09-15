"""Unmodified HiveMind HEAD with explicit loopback comparison config.
For no-wait use --rpm 1000000 --tpm 1000000000: enabled but nonbinding.
"""
import argparse
from hivemind.storage.models import HiveMindConfig
from hivemind.proxy.server import run_proxy
parser = argparse.ArgumentParser()
parser.add_argument("--port", "--listen-port", dest="port", type=int, default=18765)
parser.add_argument("--mock-port", type=int, default=18764)
parser.add_argument("--rpm", type=int, default=16)
parser.add_argument("--tpm", type=int, default=6000)
args = parser.parse_args()
assert 1024 <= args.port <= 65535 and 1024 <= args.mock_port <= 65535
assert args.rpm > 0 and args.tpm > 0
run_proxy(HiveMindConfig(
    proxy_host="127.0.0.1", proxy_port=args.port,
    upstream_url=f"http://127.0.0.1:{args.mock_port}",
    max_concurrency=2, min_concurrency=2, max_retries=0,
    rpm_limit=args.rpm, tpm_limit=args.tpm, rate_limit_scope="global", max_rate_wait_s=120,
    telemetry_dsn=None,
))
