# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-028"
title: "Multi-Peer E2E Simulation & Mesh Snapping Validation Suite"
state: "PLANNING"
roadmap: "multi-user-mesh-chat"
depends_on: ["ticket-027"]
created_at: "2026-10-18T21:02:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-028/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/multi-user-mesh-chat.md`

## 1. Problem Statement & Objective
With multi-peer mesh connections, in-band relay signaling, and header add-peer invite controls implemented, comprehensive end-to-end integration tests must validate the full multi-user mesh lifecycle under simulated real-world conditions:

1. Validate 3+ peer mesh establishment (Alice, Bob, Charlie).
2. Validate in-band relay snapping between Bob and Charlie with 0 edge hits.
3. Validate multi-hop message delivery, broadcast flooding, and loop suppression across triangular/cyclic meshes.
4. Validate partial peer departure (e.g. Bob departs; Alice and Charlie retain communication and selectively purge only Bob's messages).
5. Validate full workspace regression suite and zero compiler/clippy warnings.

---

## 2. Scope Boundaries
- **In-Scope**:
  - `zero_client/tests/mesh_simulation_test.rs`:
    - Integration tests covering 3+ node mesh snapping, gossip flooding, partial peer departures, and dynamic peer counter synchronization.
  - `zero_client/src/ui.rs`:
    - Unit tests validating dynamic multi-peer list management, selective purging, and explicit session destroy.
- **Out-of-Scope**:
  - Live production Cloudflare deployment.

---

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-028.1 (Multi-Node Connected Mesh Simulation Test)**:
  - Integration test verifies 3+ node mesh establishment and direct channel snapping.
- [ ] **REQ-028.2 (Partial Peer Departure & Feed Survivability)**:
  - Verifies that when 1 node departs a 3-node mesh, surviving nodes continue communicating and purge only the departed peer's messages.
- [ ] **REQ-028.3 (Full Workspace Test Suite & Linter Integrity)**:
  - `cargo test --workspace` passes with 100% success.
  - `cargo clippy --workspace --all-targets -- -D warnings` reports 0 warnings.
  - `cargo check --target wasm32-unknown-unknown -p zero_client` is clean.
