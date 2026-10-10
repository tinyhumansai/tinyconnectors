# Module processing and leased archives

Contract 1.13 makes the remaining connector algorithms callable without linking
the implementation. Hosts choose the IANA time zone and task-recency boundary
and apply product policy before executing external actions. `PrepareArguments`
applies calendar defaults without overwriting supplied values, injects verified
task-window arguments, then normalizes and validates the result. Invalid context
or arguments return structured validation failures. `FilterResponse` uses the
existing verified provider shapes and clears stale formatted Markdown only when
rows are removed. Unknown provider shapes retain existing pass-through behavior.

`ClassifyError` returns the existing stable class identifier and user-facing
message. Provider content can appear in that message and is never suitable for
telemetry. Existing Execute and trigger operation wire representations remain
unchanged; new callers can obtain structured classification separately.

`OpenArchive` receives a host-validated state directory and returns an opaque
random lease. `RecordTrigger` and `ReadArchive` operate on that lease and preserve
the existing daily JSONL format, UTC names, locking and history metadata.
`CloseArchive` releases the lease without deleting files. Module shutdown drops
the table. At most 64 leases can be open; unknown and closed handles fail. Hosts
drain the old identity's operations, close its archive lease, and open the new
identity's directory when switching users. Credential changes use the existing
Configure operation. Load-time ListTriggerHistory remains unchanged.

File operations run on blocking workers. Archive faults crossing the new surface
use static messages that exclude paths and event content. Integration requires
a compatible released artifact and verified digest before changing host calls.
Tests use local fixtures, mocked routes and the real in-memory bus; release
verification calls preparation and classification inside the dynamic artifact.
