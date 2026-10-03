Official participant brief for two tracks: **P1 â€” Compact tool schemas** and **P2 â€” Request classifier**. Build a working, tested contribution to Nasikoâ€™s public OSS repository.

## Common submission requirements
- Choose one track. Deliver a working one-day slice as a merge-ready **fork PR** to `Nasiko-Labs/nasiko`, with changes scoped to `llm-router/` (plus tests). Compact tool schemas may also add one new crate `tool-compact/` at the repo root and its entry in the root `Cargo.toml` `[workspace] members`. No `submissions/` folders; the PR is the submission.
- PR title prefix: `[compact-tools]` (P1) or `[classifier]` (P2). Use the slug, not P1/P2, in PR titles and code.
- Keep existing behavior as the default; make experimental behavior opt-in. Never include API keys, private prompts, or sensitive user data.
- Inspect existing code before introducing duplicate components.

## How we run your submission
- We run a cargo example from your PR directly; no `justfile` needed.
  - Compact tool schemas: `cargo run --release -p nasiko-llm-router --example compact_tools_eval`; see that track's **How we run it**.
  - Request classifier: `cargo run --release -p nasiko-llm-router --example classifier_eval` (example at `llm-router/examples/classifier_eval.rs`).
- Contract:
  - No required arguments. Optional env vars allowed (e.g. `MODELS`), documented in the PR.
  - Dataset path via `EVAL_SET` env var (download the public sample with `curl`).
  - Output path via `OUT` env var: one JSONL line of outputs per case. Report outputs, not scores.
  - Exit code 0 = eval ran. Our scorer computes all metrics and the pass/fail verdict.
  - Runs within 15 minutes on a CPU runner. Network only for a hosted backend or live model you configure through env vars (see each track).
- We run the same command against a private scoring set on isolated runners. **Our harness numbers decide ranking**; numbers in your PR description are claims only and are checked against ours.

Neither track writes `results.json`; each track's **How we run it** gives its JSONL output format.

### PR description template
- Track
- How to run (any env vars)
- Model IDs used, if you ran live models (optional for compact tools)
- Your measured results
- Known limits and unsupported cases

## Judging
- Merge-ready contribution 40 Â· usefulness 25 Â· code, tests, docs 20 Â· demo 15. First-time contributors get a bonus.
- Scored automatically: eval runs, tests pass, metrics on the private set.
- Scored by judges: merge readiness, usefulness, code quality, demo. Top results are reviewed by hand for overfitting or hard-coded answers.
- Report measured results honestly; targets are not claims of existing performance.

## P1 â€” Compact tool schemas without breaking tool calls
**Problem:** Tool definitions consume prompt tokens. Build a compact tool format and decoder that reduces this overhead while preserving the tool names and arguments returned to clients. Wiring it into the router as an opt-in transformation is a bonus.

**Illustrative example (not a mandated format; token savings not measured)**

Client sends a native tool definition:
```json
{
  "type": "function",
  "function": {
    "name": "create_calendar_event",
    "description": "Create an event in the user's calendar.",
    "parameters": {
      "type": "object",
      "properties": {
        "title": {"type": "string", "description": "Event title"},
        "start": {"type": "string", "format": "date-time", "description": "Start time, ISO 8601"},
        "duration_min": {"type": "integer", "description": "Duration in minutes"},
        "attendees": {"type": "array", "items": {"type": "string"}, "description": "Attendee emails"},
        "visibility": {"type": "string", "enum": ["public", "private"]}
      },
      "required": ["title", "start"]
    }
  }
}
```

Router injects a compact definition plus call-format instructions instead:
```text
create_calendar_event(title:str, start:datetime, duration_min?:int, attendees?:[str], visibility?:public|private) - Create an event in the user's calendar.
To call a tool, emit: <<call name {json args}>>
```

Model replies in compact form:
```text
<<call create_calendar_event {"title":"Design review","start":"2026-10-05T15:00:00+05:30","attendees":["riya@example.com"]}>>
```

Router decodes and returns a standard tool call; the client never sees the compact format:
```json
{"tool_calls":[{"id":"call_1","type":"function","function":{"name":"create_calendar_event","arguments":"{\"title\":\"Design review\",\"start\":\"2026-10-05T15:00:00+05:30\",\"attendees\":[\"riya@example.com\"]}"}}]}
```

Edge cases, all covered by required scope: unknown tool name or missing `title` â†’ failure policy, not a guessed call; `visibility:"secret"` â†’ enum violation; marker split across stream chunks; `>>` inside a string argument â†’ escaping.

**Evaluation set format (public samples)**
The standard set is published as one JSON file (`compact-tools-eval@v1-sample`): `{schema_version, purpose, tools, cases}`. `tools` holds the full schemas (`create_calendar_event`, `send_email`); `cases` lists the cases. Each case:
```json
[
  {
    "id": "ct-001",
    "tools": ["create_calendar_event", "send_email"],
    "messages": [{"role": "user", "content": "Book a design review Monday 3pm IST with riya@example.com"}],
    "expected": [{"name": "create_calendar_event", "arguments": {"title": "Design review", "start": "2026-10-05T15:00:00+05:30", "attendees": ["riya@example.com"]}}]
  },
  {
    "id": "ct-002",
    "tools": ["create_calendar_event", "send_email"],
    "messages": [{"role": "user", "content": "Email sam@example.com that the build is green, and add a private 30 min retro tomorrow 10am IST"}],
    "expected": [
      {"name": "send_email", "arguments": {"to": ["sam@example.com"], "subject": "Build status", "body": "The build is green."}},
      {"name": "create_calendar_event", "arguments": {"title": "Retro", "start": "2026-10-04T10:00:00+05:30", "duration_min": 30, "visibility": "private"}}
    ],
    "match": {"free_text_fields": ["subject", "body", "title"]}
  },
  {
    "id": "ct-003",
    "tools": ["create_calendar_event"],
    "messages": [{"role": "user", "content": "What's the weather?"}],
    "expected": []
  }
]
```
Cases reference tools by name from the file's `tools` array. `free_text_fields` are checked for presence and type only.

The file also has `decoder_cases`: raw model output split into stream `chunks`, with the expected calls or error. Example:
```json
{"id": "dc-002", "note": "marker split across stream chunks", "tools": ["create_calendar_event"],
 "chunks": ["<<ca", "ll create_calendar_event {\"title\":\"Ret", "ro\",\"start\":\"2026-10-04T10:00:00+05:30\"}>", ">"],
 "expected": {"calls": [{"name": "create_calendar_event", "arguments": {"title": "Retro", "start": "2026-10-04T10:00:00+05:30"}}]}}
```
Error cases expect `{"error": "unknown_tool"}` or `{"error": "invalid_arguments"}`. If your grammar differs from `<<call ...>>`, your example must convert each decoder case automatically (render the case's call in your grammar, split at the same relative positions as our chunks), never by hand per case, because the private cases are unseen. Say so in the PR. The private set uses the same schema. Everyone is scored on the same set; if you add your own cases, report their results separately in the PR.

Live runs use a fixed reference time: today is `2026-10-02`, timezone `Asia/Kolkata`. Put this in the system message so relative dates ("Monday 3pm") resolve the same way for everyone.

**What matters most**
The core win is a compact format that (a) cuts tokens and (b) a real LLM reliably writes back correctly, plus a decoder that turns it into standard tool calls. Full router integration is a bonus, not a requirement.

**What your PR contains**
1. **Required â€” new crate `tool-compact/`** (package `nasiko-tool-compact`): pure library, no IO, no env reads, no provider code. It must not depend on `nasiko-llm-router` (the router depends on it), so it defines its own `ToolDef` / `ToolCall`; the router converts at the seam. Public API equivalent to:
   ```rust
   pub fn encode_tools(tools: &[ToolDef]) -> Result<CompactTools>;
   pub fn decode_calls(text: &str, tools: &[ToolDef]) -> Result<Vec<ToolCall>>;
   pub struct StreamDecoder { /* incremental; handles split markers */ }
   ```
   Decoding validates every call against the original schema and returns an error, never a guessed call. Router types to convert from/to (`llm-router/src/ir/chat.rs`): `ToolDef { kind, function: FunctionDef { name, description: Option, parameters: Option<Value> /* JSON Schema */ }, extra }` and `ToolCall { id, kind, function: FunctionCall { name, arguments: String /* JSON string, OpenAI shape */ }, extra }`; streaming uses `ToolCallDelta { index, .. }`. The router assigns `id`. Unit and property tests live in the crate.
   Optional but recommended: `pub fn decode_tools(compact: &CompactTools) -> Result<Vec<ToolDef>>`, which lets us check automatically that schema information survived.
2. **Required â€” eval example `llm-router/examples/compact_tools_eval.rs`** (contract below). It may call the crate directly; it does not need router wiring.
3. **Bonus â€” opt-in wiring in `llm-router`:** off by default; flag read in the binary's `config.rs`; with the flag off, behaviour is byte-identical to today, proven by a test. Partial wiring (e.g. OpenAI non-streaming only) is welcome; document what is covered.

**How we run it**
```sh
curl -fsSL https://registry.nasiko.dev/r/nasiko/compact-tools-eval -o /tmp/compact-tools-eval.json
EVAL_SET=/tmp/compact-tools-eval.json OUT=/tmp/out.jsonl \
cargo run --release -p nasiko-llm-router --example compact_tools_eval
```
- Default mode is offline: no network, no API keys, no LLM call. Deterministic: we run it twice and diff `OUT`. 15-minute limit on a CPU runner. Fetch dependencies ahead of time (`cargo fetch`); the run has no network.
- Reads `EVAL_SET`; writes one JSONL line per case to `OUT`. Report outputs, not scores:
  ```json
  {"id":"ct-001","compact_request":{},"compacted":true,"rendered_calls":"<<call ...>>","roundtrip_calls":[]}
  {"id":"dc-002","decoded":{"calls":[]}}
  ```
  `compact_request` is the full OpenAI-shaped chat request body you would send (messages, tools or injected definitions, call-format instructions); `compacted: false` means you bypassed compaction for that case; `rendered_calls` is the expected calls written in your format; `roundtrip_calls` is that text decoded back; `decoded` is your `StreamDecoder` result, fed chunk by chunk. We build the native baseline request ourselves from `tools` and `messages`, so you do not output it.
- **Live mode (for format adherence):** when `PROVIDER_BASE_URL` and `MODEL` are set, send each `compact_request` to that OpenAI-compatible endpoint at temperature 0 and add `raw_output` (the model's text) and `live_calls` (your decoded result or error) to the line. We point this at our proxy, which holds the keys. You may test it with your own key; never commit keys.
- Measure locally with `tiktoken-rs` (pinned version, `o200k_base`) as a `dev-dependency` of the example only, never of the library or router.

**Required scope**
1. Encode tools compactly with call-format instructions, and decode model output (including multiple calls, text before or after a call, and plain answers with no call) into standard OpenAI-shaped tool calls.
2. Write down an explicit grammar. Test escaping, malformed output and markers split across stream chunks.
3. Keep schema meaning: required vs optional fields, types, enums, nested objects and arrays. Descriptions may be shortened but not removed where they disambiguate. Document unsupported schema features and bypass compaction for them.
4. Fail closed: an unknown tool, missing required field or invalid value returns an error, never a guessed or silently altered call.

**Stretch (bonus)**
- Router wiring (above), streaming through the router, Anthropic endpoints, conversation history with previous calls/results, forced `tool_choice` (bypass compaction when it cannot be guaranteed).

**Acceptance and evaluation**
Our scorer reads `OUT` and recomputes everything; numbers you print are ignored. Evaluation has three steps.

1. **Screening checks (automatic, not exact).** These filter out broken runs; borderline results go to review, not straight to rejection.
   - *Round trip:* `roundtrip_calls` match the expected calls (tool name and arguments; key order ignored; `free_text_fields` checked for presence and type only).
   - *Decoder cases:* most `decoder_cases` give the expected calls or error, and no error case produces a guessed call.
   - *Schema kept:* if you provide `decode_tools`, its output matches the original schemas; otherwise we check by review.
   - Prompt-only compaction without a decoder does not pass screening.
2. **Token reduction (automatic).** `1 - sum(compact tokens) / sum(baseline tokens)` over all cases, counted by us with `o200k_base` on the full request body. Bypassed cases count as 0% savings. Target â‰¥30% (a target, not a gate).
3. **Live format adherence and review (us, for runs that pass screening).** We run live mode through our proxy on at least two models from different providers and measure how often the model writes valid compact calls that decode to the right tool and arguments, compared with native tool calling. We also review the grammar, schema preservation and code by hand or with LLM review. A format that saves tokens but models cannot follow ranks low.

Ranking weighs token reduction and live format adherence together; neither alone wins.

**Boundaries:** Only requests already passing through `llm-router`. No gateway protocol changes, direct-provider interception, or default configuration changes. Gateway catalog hygiene and orchestrator tool-subset selection are optional stretch work, scored separately. Check existing `llm-router/src/brevity.rs` and the `nasiko-compress` crate (`compress/`, already used by `llm-router/src/compress.rs`) before starting; follow its fail-closed, deterministic invariants.

## P2 â€” Request classifier for cost-aware routing
**Problem:** The routerâ€™s regex classifier uses keywords to identify request type; it misses differences in complexity and context. Build and evaluate a model-agnostic decision interface, without assuming a decision model improves routing.

**Evaluation set format (public samples)**
We publish a 10-case smoke set as JSON (`classifier-eval@v1-sample`, also tagged `0.1.1`); it is not evidence of improvement. The file includes the label set and complexity rubric. `request_type` is one of `code_generation`, `code_understanding`, `technical_design`, `analytical_reasoning`, `writing`, `factual_lookup`, `general` (the router's `RequestType`). Example case:
```json
{"id": "pub-01", "query": "Fix typo in this Python comment: `# retrun the cached value`.", "context": "No other files or changes needed.", "request_type": "code_generation", "complexity": 1, "tier_hypothesis": "tier_3", "tests": ["short_request", "short_context", "code_keyword_low_effort"]}
```
`tier_hypothesis` is illustrative, not ground truth. Download with `curl` (no login, no extra tools):
```sh
curl -fsSL https://registry.nasiko.dev/r/nasiko/classifier-eval -o /tmp/classifier-eval.json
``` 

The private set uses the same schema.

**How we run it**
```sh
EVAL_SET=/tmp/classifier-eval.json OUT=/tmp/classifier-out.jsonl \
cargo run --release -p nasiko-llm-router --example classifier_eval
```
- The repo ships `llm-router/examples/classifier_eval.rs` as the regex baseline. Extend it to call your classifier; keep the I/O contract.
- Reads `EVAL_SET` (`examples` array); writes one JSONL line per case to `OUT`:
  ```json
  {"id":"pub-01","request_type":"code_generation","complexity":1,"confidence":0.92,"latency_us":840}
  ```
  `confidence` in [0,1]; `complexity` 1â€“5. Load the model once before the loop; per-call `latency_us` excludes load. Our scorer also times the full run; your `latency_us` is a cross-check.
- Backend is chosen by env vars you document (e.g. `CLASSIFIER_BACKEND=regex|local|hosted`, `CLASSIFIER_MODEL_PATH`, `CLASSIFIER_ENDPOINT`). Local backends run with no network. Hosted backends run through our egress proxy, which only allows the endpoint you name in the PR; we supply keys, never commit yours. We run it twice and diff outputs for determinism.

**Required scope**
1. Implement a typed interface equivalent to `classify(query, context) -> {request_type, complexity, confidence}`. Define request-type mapping, complexity 1â€“5, and confidence semantics. Today `classify_request_type(text: &str) -> RequestType` takes query text only; add a `RequestClassifier` trait in `llm-router/src/routing/classifier.rs` with the regex as the default implementation (behaviour unchanged when off), backend selected by configuration (local model or hosted API), and regex fallback on model-load, inference or network failure, or timeout. Backend choice, model path, endpoint and timeout are read in the binary's `config.rs`, never in the library.
   Trait shape (equivalent signatures are fine; async because hosted backends make network calls):
   ```rust
   pub struct ClassifyInput<'a> { pub query: &'a str, pub context: Option<&'a str> }
   pub struct Classification { pub request_type: RequestType, pub complexity: u8 /* 1-5 */, pub confidence: f32 /* 0-1 */ }
   #[async_trait::async_trait]
   pub trait RequestClassifier: Send + Sync {
       fn name(&self) -> &str;
       async fn classify(&self, input: &ClassifyInput<'_>) -> Result<Classification, ClassifyError>;
   }
   ```
   The router holds an `Arc<dyn RequestClassifier>`. The regex implementation wraps `classify_request_type` and documents its fixed `complexity` and `confidence` values. On `Err` or timeout, the router uses the regex result and counts a fallback. `classifier_eval` calls the same trait, so the eval exercises the same code path as the router.
2. Choose your classifier freely: embeddings plus a small classifier, distilled transformer, or another compact decision model. [Jev](https://typesafe.ai/blog/introducing-system-one-models-and-jev) and [Laya](https://github.com/receptron/laya) are candidates, not requirements. Compare the regex baseline against at least one backend of your choice. Local and hosted backends are equally welcome: users of the OSS product pick what fits them through config. The regex stays the out-of-the-box default, so nothing needs a network unless the user opts in.
3. Build your own labelled train and validation data with documented labelling criteria, keeping near-duplicates out of any split. We hold a private set of about 200 labelled queries for final scoring; it is never published. It deliberately includes ambiguous, multi-intent, paraphrased, out-of-distribution, and noisy/padded queries, plus near-miss cases the regex gets wrong.
4. Preserve provider-specific cost-aware tier mapping. Classify only at safe routing boundaries (`cold_start`, `switch`); keep the selected tier sticky during `continue` tool-loop steps. Do not move classification into the orchestrator.
5. Deterministic classification and tier selection for identical inputs/state. If sampling is included, expose a seed and report variance. If the bandit key includes complexity, document the mapping and feedback simulation. Specify safe low-confidence behavior.

**Costs we measure (reported, not gates)**
- p50/p95 decision latency per query, end to end (network included for hosted backends), excluding one-time model load.
- Cost per decision (API price for hosted; hardware for local), model size and cold-start load time for local.
- Fallback rate on timeouts or errors.
The classifier runs before routed requests at each boundary, so its latency and cost are weighed against the routing gain. A slow or expensive classifier is not rejected, but ranks lower unless its accuracy gain pays for it.


**Acceptance and evaluation**
- Primary metric: request-type accuracy on the private set vs. the regex baseline. Confident wrong answers are penalised: we report expected calibration error (ECE), and low-confidence cases routed to the safe default count as fallback, not error.
- Also report your own held-out results, complexity performance (with your rubric), p50/p95 latency, hardware, load time, and cost per decision.
- Demonstrate configurable backend selection, regex fallback on failure or timeout, reproducibility, and sticky-routing tests. Show failures and negative results as well as wins.
- To claim cheaper or better routing, measure downstream answer quality, model usage, tokens, and cost against the existing router on the same workloads. Accuracy alone does not establish routing quality or savings. No mandatory token-reduction target.

**Stretch (scored separately):** tool preselection before compaction, stop/continue decisions in the tool loop, per-request reasoning budget.

**Boundaries:** Request classification and its tier-routing integration only. Flow-branch decisions, agent-selection redesign, and internal TokenOps dashboards are out of scope.

## Demo checklist
- Run baseline and your implementation: `cargo run --release -p nasiko-llm-router --example compact_tools_eval` or `cargo run --release -p nasiko-llm-router --example classifier_eval`.
- Show aggregate metrics, at least one success, and at least one failure/fallback.
- P2: state which backend you ran, plus its p50/p95 latency and cost per decision.
- Explain what is complete, what is unsupported, and what remains before production adoption.

