# Requests between agents

Rules (see CLAUDE.md §2.4): only the requester creates an entry; only the receiver
changes **Status** and writes **Response**. Never delete entries. Number entries
sequentially (REQ-1, REQ-2, …) regardless of direction.

Template:

```
## REQ-<number>: <short title>
- From: ui | backend
- Date: YYYY-MM-DD
- Status: open | accepted | done (SPEC vX.Y.Z) | rejected (reason)
- Need: what is needed and why (which screen/state)
- Proposed shape: optional
- Response: (filled in by the other agent)
```

---

<!-- New entries below this line, newest last. -->
