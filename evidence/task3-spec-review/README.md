# Task 3 independent spec review evidence

Reviewed the product source listed in reviewed-source-sha256.json. Product files were not edited.

`cargo +1.88.0 test --locked` independently passed 75 tests (24 stream tests) against the reviewed final source; its raw result is in the review tool output, chunk 5570ca. The implementer historical log was not substituted for that run. The corrected historical RED log compiled and then reported 3 passes / 10 behavioral failures.

Run `python3 evidence/task3-spec-review/reproduce.py` from the research workspace after a locked build with Rust 1.88.0. It compiles temporary binaries against existing build dependencies, runs only synthetic loopback fixtures, and does not call real services. Preserve/verify the reviewed source hash before comparing results.

- observer-capacity contains the exact SseDecoder source extracted from the reviewed stream.rs with a no-op endpoint observer so allocated Vec capacities can be inspected. The decoder implementation is unchanged; this is a direct decoder-memory proof, not an RSS benchmark.
- socket-deadline-config reuses the actual stream fixture and actual compiled gateway library. The saturated-deadline test intentionally fails because truncated HTTP 200 has a successful chunked terminator. Config checks print actual spawn acceptance for 0/17/255. These are temporary synthetic gateways only.
- usage reuses actual socket fixtures and compiled gateway. It prints directly observed status for cached=7, malformed cached=-7, and decreasing Chat usage.

Raw outputs are retained beside each source. Existing tests included in review.rs are filtered out by `review_`; only the additional review cases execute.
