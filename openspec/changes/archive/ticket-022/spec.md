# OpenSpec SDD: Task Specification

```yaml
---
ticket_id: "ticket-022"
title: "Multi-Peer Mesh Integration & E2E Handshake Validation for Leave and Burn Lifecycle"
state: "IN_PROGRESS"
roadmap: "improved-leave-burn-lifecycle"
depends_on: ["ticket-021"]
created_at: "2026-10-18T17:05:00Z"
base_branch: "main"
ticket_branch: "factory/ticket-022/main"
max_review_cycles: 3
---
```

Roadmap: `openspec/roadmaps/improved-leave-burn-lifecycle.md`

## 1. Problem Statement & Objective
With protocol signaling (`ticket-020`) and client UI bindings (`ticket-021`) implemented, end-to-end integration validation across multi-peer mesh simulations and loopback handshakes is necessary to prove correctness under concurrent conditions.

This task adds comprehensive integration test suites verifying:
1. Multi-peer selective message purging on peer departure in a 3+ node mesh topology.
2. Multi-peer global room annihilation when any node triggers a session burn.
3. Verification of zero regressions across all existing protocol, cryptographic, and network test suites.

---

## 2. Scope Boundaries
- **In-Scope**:
  - `tests/mesh_simulation_test.rs`:
    - Add test: `test_req_leave_and_burn_selective_message_purging`:
      - Alice, Bob, and Charlie establish a 3-node mesh.
      - Each sends multiple encrypted chat messages.
      - Bob initiates `leave_and_burn`.
      - Verify that Alice and Charlie receive the signal and drop all of Bob's messages from memory/routing.
      - Verify that Alice and Charlie can continue exchanging messages seamlessly.
    - Add test: `test_req_global_burn_session_annihilation`:
      - Alice, Bob, and Charlie establish a 3-node mesh with active message feeds.
      - Any participant initiates `burn_session`.
      - Verify that all three nodes destroy their sessions, sever all channels, and zeroize state.
  - Full regression validation:
    - Run `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`.
- **Out-of-Scope**:
  - Production deployment or Cloudflare edge modifications.

---

## 3. Requirements & Acceptance Criteria
- [ ] **REQ-022.1 (Multi-Peer Selective Message Purge Integration Test)**:
  - An automated integration test in `tests/mesh_simulation_test.rs` must verify that when one peer leaves in a 3+ node mesh, their messages are purged from other nodes while remaining peers retain their chat history and communication links.
- [ ] **REQ-022.2 (Multi-Peer Global Burn Integration Test)**:
  - An automated integration test in `tests/mesh_simulation_test.rs` must verify that triggering a global burn terminates all DataChannels and purges all message feeds across all participants.
- [ ] **REQ-022.3 (Full Workspace Non-Regression Integrity)**:
  - All unit and integration tests across the entire Cargo workspace pass cleanly.
  - Clippy reports zero warnings across all targets.
