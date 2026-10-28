# `syntax.match_arrow`

```jadpo check project=e-match-arrow expect=syntax.match_arrow
type TicketStatus = Enum {
    open
    closed
}

type Rank = Int {
    min: 1
}

function rank(status: TicketStatus) -> Rank {
    return match status {
        TicketStatus.open => 1
        TicketStatus.closed -> 2
    }
}
```

```text output
error: Jadpo writes `->`, not `=>`
 --> main.jadpo:12:27
   |
12 |         TicketStatus.open => 1
   |                           ^^ write `->`
  reason: `=>` ends a match arm's pattern in Rust, Scala, C# and PHP; a Jadpo arm uses `->`, the same arrow as a result type.
  next: Use `->` instead of `=>` (agent)
  rule: syntax.match_arrow
  help: docs/errors/syntax.match_arrow.md

check failed: 1 error, 1 file
```
