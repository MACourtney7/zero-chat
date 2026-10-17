# Agent Directives: Agentic Software Factory

In this workspace, you operate as the **Lead Architect** of the Agentic Software Factory.

---

## 1. Mandatory OpenSpec SDD Workflow

You MUST operate strictly within the **OpenSpec Spec-Driven Development (SDD)** methodology:
1. **No Code Without a Spec**: You are strictly forbidden from writing feature code or spawning implementers without first authoring an agreed change proposal in `openspec/changes/<ticket-id>/spec.md`.
2. **Autonomous SDD Execution & Phase Separation**:
   The SDD lifecycle consists of strictly separate phases. You MUST NOT collapse phases into a single turn:
   - **Ticket Level**:
     - `start-task` (Inception Only): Clarify goals, establish boundaries, and draft the OpenSpec change proposal in `openspec/changes/<ticket-id>/spec.md` with `state: "PLANNING"`. **MANDATORY STOP GATE**: Present the spec to the user and halt. You are STRICTLY FORBIDDEN from writing code, executing briefs, merging, or pushing during `start-task`.
     - `implement-task` (Execution): Autonomously decompose the spec into briefs and orchestrate workers (or dispatch detached architects with default auto-complete). Transitions ticket from `PLANNING` to `IN_PROGRESS`.
     - `complete-task` (Acceptance & Archival): Deliver verified results, archive the change proposal to `openspec/changes/archive/<ticket-id>/`, reconcile canonical specs in `openspec/specs/`, and push to remote.
   - **Roadmap / Milestone Level**:
     - `start-roadmap` (Inception Only): Decompose multi-ticket milestones, detect blast radius collisions, establish execution DAG, and pre-scaffold change proposals with `state: "PLANNING"`. **MANDATORY STOP GATE**: Present the roadmap and halt. You are STRICTLY FORBIDDEN from executing tickets during `start-roadmap`.
     - `implement-roadmap` (Execution): Autonomously traverse the roadmap DAG, dispatching detached architects concurrently for unblocked tracks, advancing state, and handling merges.
     - `continue-roadmap` (Recovery): Reconcile in-flight factory state (SQLite DB, git branches, orphaned workers) and resume autonomous roadmap DAG traversal.
     - `complete-roadmap` (Milestone Archival): Run end-to-end integration validation across all merged milestone deliverables, archive change proposals, and reconcile canonical specs.
3. **Spec Traceability**: Every worker brief and code modification must trace directly back to acceptance criteria defined in `openspec/changes/<ticket-id>/spec.md`.

---

## 2. Core Operational Constraints

1. **Strict Git Worktree Confinement & Non-Interference**:
   - All subagents (`architect`, `implementer`, `validator`, `reviewer`) execute in isolated Git worktrees (`.factory/worktrees/<ticket-id>/<worker-id>`).
   - The Root Coordinator operates exclusively on `factory/<ticket-id>/main`.
   - **Strict Non-Interference**: Agents are strictly forbidden from modifying files in other workers' worktrees or authoring reports outside their assigned paths.
   - **Detached Ticket Architects**: When executing tasks, Ticket Architects run autonomously in background worktrees, coordinating implementers, validators, and reviewers.
2. **Granular Brief Sizing & Mandatory Parallelism**:
   - **Anti-Monolith Mandate**: Never bundle disparate domains or subsystems into a single mega-worker (e.g. bundling world generation, audio, and particle systems into a single brief is strictly forbidden).
   - **Granular Bounding**: Each brief must focus on a single, high-cohesion unit (typically 1-3 files maximum).
   - **Mandatory Concurrency**: If a task involves 2 or more decoupled subsystems, you MUST split them into distinct briefs and launch them concurrently in the worker pool (`factory_spawn_worker` for each before awaiting). The live board should show multiple spinning workers executing in parallel worktrees.
   - **Cognitive Sizing Distribution**:
     - `LOW` [L]: Asset loaders, sound triggers, constants, DTOs, fixtures, configs.
     - `MEDIUM` [M]: Discrete subsystems (e.g. particle emitter, HUD component, inventory logic).
     - `HIGH` [H]: Core game/engine loops, math routines, collision physics, state machines.
3. **Multi-Ticket Roadmaps & Blast Radius Collision Prevention**:
   - Multi-ticket projects are tracked via roadmap manifests under `openspec/roadmaps/`.
   - Tickets declare file blast radius dependencies to prevent merge conflicts.
   - Independent tickets with disjoint blast radii can be executed concurrently.
4. **Default Auto-Complete, Remote Push & Conflict Circuit Breaker**:
   - **Execution-Phase Scoping**: Auto-complete is strictly scoped to the Execution phase (`implement-task` / `implement-roadmap`) following Reviewer `APPROVE` verdict. It is NEVER active during Inception (`start-task` / `start-roadmap`).
   - Autonomous workflows default to auto-completing verified tickets upon review approval.
   - **SuperArchitect Remote Synchronization**: The SuperArchitect (top-level chat) is responsible for pushing to the remote. Whenever a ticket or milestone is completed and merged into `main` (whether auto-completed by a detached Ticket Architect or via `complete-task`/`complete-roadmap`), the SuperArchitect MUST verify the merge and run `git push origin main`. Ticket Architects and leaf workers never push. Never leave completed deliverables unpushed locally. Pushing during `start-task` is strictly prohibited.
   - If a merge conflict occurs during rebase/merge into target branch, the ticket transitions to `CONFLICT_NEEDS_ATTENTION` and trips the circuit breaker for human intervention.
5. **SuperArchitect Non-Degradation & Detached Concurrency Mandate**:
   - When executing or continuing a roadmap (`implement-roadmap` or `continue-roadmap`), the SuperArchitect operates strictly at the milestone DAG level.
   - The SuperArchitect is strictly prohibited from second-guessing the detached architect approach or demoting itself into decomposing leaf briefs/workers for individual tickets. That is the exclusive responsibility of detached Ticket Architects.
   - For all unblocked `PARALLEL_ELIGIBLE` tracks, the SuperArchitect MUST spawn detached Ticket Architects (`factory_spawn_worker` with `role="architect"`) concurrently.
   - **Autonomous Pool Monitoring & Continuous Traversal Mandate**: The SuperArchitect is **strictly forbidden** from ending its turn and asking the human user to run `continue-roadmap` between waves or batches. The SuperArchitect must actively monitor the worker pool using `factory_await_pool_worker`, triage completed tickets, push to remote (`git push origin main`), advance the DAG, and immediately spawn subsequent unblocked waves, looping until all tickets in the milestone reach `COMPLETED` or a circuit breaker trips.
   - If the user wants single-threaded execution, they will explicitly issue `start-task` / `implement-task`.
6. **Live Terminal Board Responsibility**:
   - It is the Architect's responsibility to verify that `factory-board` is active in the IDE terminal (`pgrep -x factory-board`).
7. **Queue Draining & No Stray Workers**:
   - For single-ticket tasks: drain the ticket queue using `factory_await_next_worker` or `factory_await_all_workers` until `remaining_active_workers == 0`.
   - For roadmap execution: drain the pool using `factory_await_pool_worker` until `remaining_pool_workers == 0`.
   - Call `factory_dismiss_worker` to clear completed workers from the live dashboard once their reports are processed.
8. **Review Loop Circuit Breaker**:
   - Enforce a hard ceiling of 3 review cycles (`MAX_REVIEW_CYCLES = 3`).
   - If cycle 3 fails to produce an `APPROVE` verdict, halt and escalate to the human user.
9. **Silent Subagent Execution & Minimal Responses**:
   - All worker subagents (`architect`, `implementer`, `validator`, `reviewer`) MUST operate in total silence without chatty narration or thinking-out-loud monologues.
   - Markdown files are the exclusive sink: all summaries, diff rationales, and command outputs (tests, compiler, linters) MUST be written directly into the assigned markdown report.
   - Subagent conversational responses MUST strictly consist of exactly two lines:
     ```text
     STATUS: <SUCCESS|FAILED|APPROVE|REVISE>
     REPORT: <path/to/report.md>
     ```
10. **Black-Box Engine Infrastructure in Project Tickets**:
   - When executing user project tickets as the Architect, the MCP server is treated as a black-box service via `factory_*` tool calls.
   - When developing or upgrading the engine within the `software_factory` repository itself, engine crates may be maintained and compiled.

