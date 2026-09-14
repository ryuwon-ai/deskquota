# Task 3 independent spec re-review

SPEC PASS for the four first-round findings and held Task 3 scope. No additional actionable spec defect found in the revised receiver or fixed decoder.

Reviewed source SHA256 values and binary identity are in reviewed-source-sha256.json. Binary: 6128b81969201b3bd4fb4b3af0b56e6f4da8ee5c085efdd411c73f520d6d15df.

Independent cargo +1.88.0 test --locked passed all 83 tests; raw stdout/stderr is independent-locked-test.log. The fix RED log was read: successful compilation followed by cache/concurrency/saturated-wire failures and an allocated-capacity 331072-byte failure. Those were behavioral failures, not fixture/compile substitutes.

The original first-round socket-deadline-config and usage review sources were recompiled with rustc 1.88 against the CURRENT llmgw/tokio/serde_json/socket2 rlibs from product/target/debug/deps, then run with review_ --nocapture. Their current output is retained here. First-round source/evidence remains unchanged in ../task3-spec-review.

- Saturated deadline: terminal_deadline=1, max queued payload=65536, no successful chunked terminator. Raw TCP read_to_end returning Ok only means socket EOF; the missing chunked terminator is the protocol failure expected here. Product tests additionally check transport failure through an HTTP client.
- Constructed concurrency 0, 17, 255: spawn rejected before bind. Existing suite also exercises valid limits.
- Cached=7: known=1 and cache_read=7; negative cache and decreasing usage: known=0.
- Current decoder exact source extracted into current-decoder-probe.rs (with a recording observer stub) retains 262144 buffer bytes for the original capacity attack. Framing passes every chunk size 1..102 across BOM, CR/LF/CRLF, multiline data, bare fields, comments, ignored fields, and incomplete EOF. This is allocation/framing evidence, not RSS or real-provider evidence. Compile that single file using rustc +1.88.0 --edition=2024 and run the resulting temporary binary to reproduce.

Source review confirms terminal failure uses a separate oneshot unaffected by payload-queue saturation; missing terminal signal also becomes an error, and only genuine upstream body EOF signals success. Fixed storage has no Vec growth. Existing worker ownership/drop order, forced abort+join, and production timeout configuration remain intact. Chat cache subset is not added to input tokens.

No product source/config edits, real LLM/API calls, OS services, Git mutations, or actual user-file fixtures occurred. Windows, engine abort, RSS/performance, real provider/client compatibility, and later Task 5-7 features are not certified by this review.
