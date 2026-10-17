---
name: implement-roadmap
description: Autonomously executes a multi-ticket roadmap by traversing its dependency DAG, spawning detached ticket architects in parallel for unblocked tracks, handling merges, and updating progress.
---

# `implement-roadmap` Workflow

Use this skill when the user triggers autonomous execution of a planned milestone or multi-ticket roadmap.

## Procedure

1. **Ensure Live Board is Active**:
   - Verify that `factory-board` is running (`pgrep -x factory-board`).
   - If not running, ensure it is launched in the terminal panel so all ticket workers are visible in real-time.

2. **Locate & Validate Roadmap Manifest**:
   - If a roadmap ID is provided (e.g. `implement-roadmap milestone-01`), target `openspec/roadmaps/<milestone-id>.md`.
   - If no ID is provided, scan `openspec/roadmaps/`:
     - If exactly 1 roadmap has pending tickets, select it.
     - If multiple exist, list them and prompt the user to choose.
   - Verify the roadmap file contains:
     - Section 2: Ticket Dependency Matrix (tickets, dependencies, and state: `PLANNED`, `IN_PROGRESS`, `COMPLETED`).
     - Section 3: Blast Radius & Scope Collision Matrix.
     - Section 4: Execution DAG Diagram.

3. **DAG Traversal & Parallel Dispatch Cycle**:
   - **Continuous Traversal Directive**: You MUST maintain an active, autonomous execution loop. You are **strictly forbidden** from stopping your turn and asking the user to manually run `continue-roadmap` between waves or batches.
   - While there are tickets in `PLANNED` or `IN_PROGRESS` state:
     a. **Identify Ready Tickets**:
        - Find all tickets in `PLANNED` state whose upstream dependencies (`Depends On`) are all `COMPLETED`.
        - Ensure their declared file blast radius does not overlap with any currently `IN_PROGRESS` tickets.
        - Tag eligible tickets as `PARALLEL_ELIGIBLE`.
     b. **Spawn Detached Ticket Architects Concurrently**:
        - For EACH ready ticket, ensure its branch and specs exist:
          - Ensure `openspec/changes/<ticket-id>/spec.md` is present.
          - Call `factory_create_ticket(ticket_id, title, "main")`.
        - Update ticket state in `openspec/roadmaps/<milestone-id>.md` from `PLANNED` to `IN_PROGRESS`.
        - Spawn the detached autonomous Ticket Architect worker:
          ```text
          factory_spawn_worker(
              ticket_id="<ticket-id>",
              role="architect",
              brief_path="openspec/changes/<ticket-id>/spec.md",
              thinking_level="HIGH"
          )
          ```
        - The Ticket Architect executes autonomously in `.factory/worktrees/<ticket-id>/`, coordinates implementers, validators, and reviewers, and auto-completes into `main` upon review approval. It does not push.
        - **Batch Spawning**: Dispatch ALL ready tickets before entering the pool monitoring loop.

4. **Active Pool Monitoring Loop (`factory_await_pool_worker`)**:
   - **Mandatory Pool Draining**: Immediately call `factory_await_pool_worker(timeout_secs=600)` to block until any worker in the pool completes or fails.
   - When `factory_await_pool_worker` returns:
     - **If `event == "WORKER_FINISHED"`**:
       - Check `ticket_id`, `worker_id`, `role`, and `status`.
       - If the finished worker is a Ticket Architect (`role == "ARCHITECT"`):
         - If `status == "COMPLETED"`:
           - **SuperArchitect Remote Push**: Verify the merge into `main` landed and run `git push origin main` immediately so downstream tickets branch from up-to-date `main`.
           - Update ticket status to `COMPLETED` in `openspec/roadmaps/<milestone-id>.md` (both in the `Ticket Dependency Matrix` and `Progress & Milestone Completion Criteria`).
           - Dismiss the architect worker: `factory_dismiss_worker(ticket_id, worker_id)`.
           - **Check Unblocked Downstream Tickets**: Re-evaluate remaining `PLANNED` tickets. If all dependencies for any downstream ticket are now `COMPLETED` and blast radii are disjoint, immediately call `factory_create_ticket` and `factory_spawn_worker(role="architect")` to launch it.
         - If `status == "FAILED"` or `ticket_state == "CONFLICT_NEEDS_ATTENTION"`:
           - Mark ticket as `CONFLICT_NEEDS_ATTENTION` in roadmap table.
           - Escalate to the human user with exact details from the architect's report.
     - **If `event == "POOL_DRAINED"`**:
       - Re-evaluate the entire roadmap manifest (`openspec/roadmaps/<milestone-id>.md`).
       - If all tickets are `COMPLETED`, exit the loop and proceed to Step 6.
       - If there are newly unblocked tickets in `PLANNED`, spawn them immediately and resume `factory_await_pool_worker`.
   - **Repeat**: Immediately call `factory_await_pool_worker` again until all roadmap tickets are `COMPLETED` or a conflict halts execution. **NEVER prompt the user to run `continue-roadmap` unless an unrecoverable system crash occurs.**

5. **Conflict & Circuit Breaker Handling**:
   - If a ticket encounters a merge conflict (`CONFLICT_NEEDS_ATTENTION`) or exceeds review cycle limits (`MAX_REVIEW_CYCLES = 3`):
     - Mark the ticket as `CONFLICT_NEEDS_ATTENTION` in the roadmap table.
     - Pause dependent downstream tickets.
     - Keep independent/disjoint ticket tracks running if unaffected.
     - Escalate the conflict to the human user with exact failure details.

6. **Roadmap Execution Wrap-Up**:
   - When all tickets in the roadmap reach `COMPLETED`:
     - Inform the user:
       > *"🎉 All tickets in roadmap `<milestone-id>` have successfully completed and merged into `main`! Issue `complete-roadmap <milestone-id>` to run end-to-end integration checks, archive proposals, and reconcile canonical specs."*
