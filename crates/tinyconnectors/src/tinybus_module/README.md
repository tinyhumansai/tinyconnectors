# TinyBus adapter

`ConnectorService` serves the connector contract with the module-owned route,
provider registry, preferences and daily trigger archive. `setup` registers its
object and claims the contract interface. Manifest methods, contract names and
dispatch order are tested together; no host implementation is needed to call it.

Contract 1.13 adds `PrepareArguments`, `FilterResponse`, `ClassifyError`,
`RecordTrigger`, `OpenArchive`, `ReadArchive`, and `CloseArchive`. Hosts choose
the time zone and recency boundary, authorize
external calls, and reconcile credentials through the existing `Configure`
operation. Argument normalization, calendar defaults, task filtering, provider
classification and JSONL append/locking execute inside the module. A structured
provider error carries the stable class identifier and existing product message;
it can contain provider detail and must never be sent to telemetry.

`OpenArchive` receives a host-validated state directory and returns an opaque
lease. `RecordTrigger` and `ReadArchive` use that lease; `CloseArchive` releases
it without deleting data. Leases are bounded, unknown/closed handles fail, and
module shutdown drops the table. Hosts close the old user's lease when switching
identity. Existing load-time `ListTriggerHistory` remains compatible. File I/O
runs on blocking workers; persisted formats and existing arities are unchanged.

The dynamic verifier exercises preparation and filtering with local payloads.
Hosts must wait for a published compatible artifact with a verified digest
before adopting new members. ABI exports remain owned by the TinyBus SDK.
