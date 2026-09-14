# Reserved-slot backfill experiment

Status: authorized research experiment; production policy and held release remain unchanged until measured acceptance. User requested Ponytail/ECC improvements on 2026-09-13. This narrows the already recorded utilization hypothesis, not a new product subsystem.

## Question and baseline

Can the existing starvation barrier admit useful work while preserving its protected head's local quota opportunity? Baseline is current request RR with the five-second/eight-bypass barrier. Earlier accounting ablation did not establish throughput superiority. Reuse the ledger, queue, ManualClock, native HTTP fixture, and existing benchmark transport. No new dependency, runtime service, predictor, cache, client configuration, or production setting.

## Candidate

Expose one candidate through the existing `bench-harness` feature and benchmark example only. Default executable and `Admission::new` retain existing behavior. Keep FIFO baseline available. When a valid barrier head does not fit now, consider fitting heads from other roots in existing RR order. Permit a candidate only when all of these hold:

1. Reserved accounting, startup hold over, shared cooldown over, and at least two currently free execution slots. One slot must remain unused for the protected head even if candidate execution never finishes.
2. The next future ledger debit expiry exists, is before the protected deadline, and is strictly less than one window from now. At that next event, the protected head must fit both RPM and TPM with every unstarted reservation still charged. Equality is unsafe: a candidate started now can expire and recharge larger late usage exactly at the event.
3. At the same event, head plus candidate must fit RPM and TPM. Charge the new candidate in full at that event even if its expected runtime is short. Recompute cumulatively after every actual admission. Never advance the real ledger clock to inspect a future time.
4. With known TPM, decline if any active, started request has an absent TPM expiry or one at/before that event, including zero-token/metadata entries. Also decline a zero-token candidate or existing unstarted zero-token reservation: starting it creates no TPM expiry, so a positive reported usage could create new debt before the projected event. The endpoint-agnostic ledger must not assume all zero-cost callers can only report zero. Existing reserved positive live-window debits cannot grow before expiry; unknown future recharge is not safe forecast slack.
5. Reserve room for both entries and both reservation IDs at the current time. Preserve root FIFO, barrier identity, queue bound, cancellation, deadline, nonpreemption, retry ownership, and ordinary admission checks.

This intentionally checks only the next expiry, not every future event. It can decline safe opportunities. The ceiling is the existing 8192-entry/16-root ledger scan; avoid adding a sort, periodic polling, or second ledger. The claimed invariant is conditional local ledger admission opportunity under the above known state, not GPU service time, provider acceptance, or immunity to external consumption/429s. Actual accounting is excluded from this first experiment.

## Correctness and experiment gate

Before comparison leave runnable traces for positive slack, cumulative slack exhaustion, long/unpolled candidate holding a slot, cap1, RPM/no slack, Actual mode, late-usage ambiguity, cancellation/deadline/cooldown, root FIFO and continuous small arrivals. Include heavy-light-heavy and heavy-heavy-light. Failed candidates remain recorded, not removed from denominator.

Use deterministic traces to check policy semantics, not claim wall-clock latency. Then run a bounded native HTTP pilot against the same loopback mock and transport, with retries/cache disabled. If pilot is useful, repeat at least five seeds with alternating arm order and include a held-out/no-benefit workload. Record all terminal outcomes, short/long delay, time-limited completions, attempts, source/binary/config/workload hashes, and resource context. Both arms use the same binary and differ only in benchmark policy. Keep prior artifacts immutable and use `target/backfill-experiment` and a new artifact directory.

Search budget: one candidate, no more than two correctness-driven revisions, at most 45 minutes of benchmark runtime this round, no paid API or real model. Stop for a violated invariant, unexplained regression, or no useful improvement. No production/default promotion based solely on synthetic favorable traces or one pilot. Paper/competitor superiority and stars remain unproven.
