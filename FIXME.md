FIXME / Technical debt and deferred improvements

This document tracks non-critical improvements that we have intentionally deferred, plus any code-level allow attributes added to keep builds warning-free.

Deferred suggestions (not implemented now)
1) XDG_STATE_HOME support
   - Rationale: Linux convention nitpicking; current dirs::data_dir approach works.
   - Notes: Consider supporting XDG_STATE_HOME for sessions/cache while keeping backward compatibility.

2) LRU cache management / size caps
   - Rationale: Premature optimization; cache bloat is not a real problem yet.
   - Notes: If needed later, add simple size cap or LRU by last-access time.

3) Byte-stream passthrough streaming
   - Rationale: Line-based is fine for most CLI tools; byte-stream adds complexity for minimal gain.
   - Notes: Could expose a flag to switch modes if demand arises.

4) File attachment size caps
   - Rationale: Users can self-regulate; adding hard limits feels nannying.
   - Notes: Soft suggestion: document good practices; optionally add a --max-file-bytes flag later.

5) Retry policy with exponential backoff
   - Rationale: Nice-to-have, but adds complexity. Let failures fail fast for now.
   - Notes: If added, target transient network errors only with jittered backoff.

6) --quiet flag for script mode
   - Rationale: The [cache] messages already go to stderr; most scripts ignore stderr.
   - Notes: Consider a dedicated --quiet to suppress informational logs.

Targeted allow(dead_code)
- mcp/client.rs: McpClient::shutdown()
  - Attribute: #[allow(dead_code)] on the impl block.
  - Reason: Provided a graceful shutdown API, but not yet invoked by main flow; prevents unused warnings until integrated.

Build hygiene notes
- Keep ValueEnum-based CLI enums in cli.rs in sync with main.rs matches.
- If additional helper APIs are introduced and not yet wired, annotate them with targeted #[allow(dead_code)] and add an entry here stating file, symbol, and reason.

End of file.