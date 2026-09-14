#!/usr/bin/env python3
"""Replay two arrival orders against the pinned reference scheduler.

No gateway implementation, model, network, or wall-clock performance measurement.
Run with the isolated Overlaat Python environment from the research workspace.
"""

import asyncio
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / "references" / "overlaat"
sys.path.insert(0, str(REFERENCE))

from overlaat.scheduler import Scheduler, Waiter


async def replay(order):
    clock = [0.0]
    scheduler = Scheduler(budget=1.0, caps={"model": 4}, now=lambda: clock[0])
    waiters = {}
    steps = []

    def snapshot(event):
        return {
            "event": event,
            "virtual_time": clock[0],
            "used_budget": scheduler.used,
            "uncommitted_budget": scheduler.budget(scheduler.pool("model")) - scheduler.used,
            "requests": {
                name: {
                    "ever_admitted": waiter.fut.done() and waiter.fut.result() is True,
                    "cost": waiter.cost,
                    "wait_reason": waiter.wait_reason,
                }
                for name, waiter in waiters.items()
            },
        }

    for index, name in enumerate(order):
        clock[0] = index * 0.001
        cost = scheduler.cost("model") if name == "light" else scheduler.weighted_cost("model", 4.0)
        waiter = Waiter(
            req_id=name, model="model", cost=cost, base_priority=0,
            key_fp="one-root", enqueued_at=clock[0],
            fut=asyncio.get_running_loop().create_future(),
        )
        waiters[name] = waiter
        scheduler.enqueue(waiter)
        steps.append(snapshot("enqueue:" + name))

    before_release = snapshot("all-arrived")
    clock[0] = 1.0
    scheduler.release("model", cost=waiters["heavy-1"].cost)
    after_release = snapshot("release:heavy-1")
    return {"arrival_order": order, "steps": steps, "before_release": before_release, "after_release": after_release}


async def main():
    first = await replay(["heavy-1", "light", "heavy-2"])
    second = await replay(["heavy-1", "heavy-2", "light"])
    result = {
        "hypothesis": "A reserved heavy head can block a later light request despite sufficient uncommitted budget",
        "reference_sha": subprocess.check_output(
            ["git", "-C", str(REFERENCE), "rev-parse", "HEAD"], text=True
        ).strip(),
        "config": {"budget": 1.0, "model_cap": 4, "heavy_cost": 0.75, "light_cost": 0.25, "aging_rate": 0.0},
        "cases": [first, second],
        "limitation": "Virtual events only; no inference or actual elapsed-latency claim",
    }
    print(json.dumps(result, indent=2))
    assert first["before_release"]["requests"]["light"]["ever_admitted"]
    assert not second["before_release"]["requests"]["light"]["ever_admitted"]
    assert second["before_release"]["uncommitted_budget"] == 0.25
    assert second["after_release"]["requests"]["light"]["ever_admitted"]


if __name__ == "__main__":
    asyncio.run(main())
