"""Native LiteLLM PyPI 1.100.1, isolated loopback benchmark config."""
import argparse
import json
import os
import sys
from pathlib import Path
import yaml
ROOT = Path(__file__).resolve().parent
CACHE = Path("/Users/ryuwon/Library/Caches/deskquota-reference-runtimes/2026-09-15/python")
parser = argparse.ArgumentParser()
parser.add_argument("--port", "--listen-port", dest="port", type=int, default=18766)
parser.add_argument("--mock-port", type=int, default=18764)
parser.add_argument("--rpm", type=int, default=16)
parser.add_argument("--tpm", type=int, default=6000)
args=parser.parse_args()
assert 1024 <= args.port <= 65535 and 1024 <= args.mock_port <= 65535
assert args.rpm > 0 and args.tpm > 0
config = yaml.safe_load((ROOT/"litellm-config.yaml").read_text())
params=config["model_list"][0]["litellm_params"]
params.update(api_base=f"http://127.0.0.1:{args.mock_port}/v1",rpm=args.rpm,tpm=args.tpm)
path=CACHE/"litellm-runtime"/f"config-{args.port}.yaml"
path.parent.mkdir(exist_ok=True)
path.write_text(yaml.safe_dump(config,sort_keys=False))
env = {k:v for k,v in os.environ.items() if k in ["PATH","HOME","LANG","LC_ALL","TMPDIR","SYSTEMROOT"]}
env.update({"NO_PROXY":"127.0.0.1,localhost", "LITELLM_LOCAL_MODEL_COST_MAP":"True", "DO_NOT_TRACK":"1", "OTEL_SDK_DISABLED":"true", "HF_HUB_OFFLINE":"1", "TRANSFORMERS_OFFLINE":"1", "LITELLM_MODE":"DEV"})
exe=str(CACHE/"litellm-venv/bin/litellm")
os.execve(exe,[exe,"--host","127.0.0.1","--port",str(args.port),"--config",str(path),"--telemetry","False","--num_workers","1"],env)
