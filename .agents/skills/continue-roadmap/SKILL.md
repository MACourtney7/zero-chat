---
name: continue-roadmap
description: Audits in-flight factory state, reconciles orphaned or stalled workers, and resumes autonomous execution of an in-progress roadmap DAG with detached Ticket Architects.
---

# `continue-roadmap` Workflow

Use this skill when resuming or continuing an in-flight multi-ticket roadmap after opening a fresh agent window, after an IDE or server restart, or when recovering state mid-milestone.

## Fundamental Operating Directives

1. **SuperArchitect Non-Degradation Mandate**:
   - The agent in this conversation operates strictly as the **SuperArchitect**.
   - You are **strictly forbidden** from demoting yourself into a single-ticket Architect (i.e. decomposing leaf implementer briefs, micro-managing individual worker diffs, or running sequential single-threaded tickets).
   - If the user wants single-threaded task execution, they will issue `start-task` / `implement-task`.
   - On a roadmap, you MUST dispatch detached Ticket Architects (`factory_spawn_worker(ticket_id, role="architect", brief_path=...)`) for all unblocked, parallel-eligible tickets concurrently.

2. **Detached Architect Reliability**:
   - Detached Ticket Architects running in `agy` are the standard autonomous execution engine of the software factory.
   - Do NOT second-guess the architect spawning approach or assume past process interruptions were architectural failures. (External factors like 24-hour quota limits or IDE server restarts can pause workers; they do not invalidate the architecture).

## Procedure

### Step 1: Verify Environment & Live Dashboard
1. Check that `factory-board` is active in the IDE terminal (`pgrep -x factory-board`).
   - If not running, inform the user or ensure it is running in the terminal panel.
2. Check if any background `agy` workers are currently executing (`pgrep -f agy`).

### Step 2: Audit & Reconcile In-Flight State
1. **Target Roadmap Manifest**:
   - If specified (e.g. `continue-roadmap the-downs`), load `openspec/roadmaps/<milestone-id>.md`.
   - If omitted, inspect `openspec/roadmaps/` to identify the active in-progress roadmap.
2. **Reconcile SQLite Database (`.factory/factory.db`) vs Disk State**:
   - Query tickets (`SELECT id, title, state, ticket_branch FROM tickets`).
   - Query active workers (`SELECT id, ticket_id, role, status, worktree_path FROM workers WHERE status = 'RUNNING'`).
   - For each worker marked `RUNNING`:
     - Check if its OS process is still running or if its worktree report has been written.
     - If the worktree report exists and is complete (e.g. `status: SUCCESS` or `APPROVE`), mark it `COMPLETED` and dismiss it (`factory_dismiss_worker`).
     - If the process is dead and no report was authored (e.g. from an IDE restart or killed task), mark the worker as `FAILED` or re-dispatch.
3. **Audit Git Branches & Remote Status**:
   - Run `git branch -a` to identify all `factory/ticket-*/main` branches.
   - Check if any tickets are merged into local `main` but unpushed: run `git status` and `git log origin/main..main`.
   - **SuperArchitect Push**: If completed tickets are merged into `main` but unpushed, run `git push origin main` immediately before dispatching downstream tickets.

### Step 3: Reconcile Roadmap Manifest
1. In `openspec/roadmaps/<milestone-id>.md`:
   - Verify Section 2 (`Ticket Dependency Matrix`).
   - Ensure tickets that have merged into `main` are marked `COMPLETED`.
   - Check off completed items in Section 5 (`Progress & Milestone Completion Criteria`).

### Step 4: Identify & Dispatch Unblocked Parallel Tracks
1. **Determine Ready Tickets**:
   - Inspect all remaining tickets in `PLANNED` or `IN_PROGRESS` state.
   - Find all tickets whose upstream dependencies (`Depends On`) are all `COMPLETED`.
   - Verify file blast radius: ensure candidate tickets do not have overlapping declared file paths with any other actively running tickets.
   - Mark eligible tickets as `PARALLEL_ELIGIBLE`.
2. **Dispatch Detached Ticket Architects in Parallel**:
   - For EACH unblocked `PARALLEL_ELIGIBLE` ticket:
     - Ensure `openspec/changes/<ticket-id>/spec.md` is present.
     - Ensure ticket is initialized: call `factory_create_ticket(ticket_id, title, "main")` if not already registered in SQLite.
     - Update ticket state in `openspec/roadmaps/<milestone-id>.md` to `IN_PROGRESS`.
     - Dispatch the detached Ticket Architect:
       ```text
       factory_spawn_worker(
           ticket_id="<ticket-id>",
           role="architect",
           brief_path="openspec/changes/<ticket-id>/spec.md",
           thinking_level="HIGH"
       )
       ```
   - **Mandatory Concurrency**: Dispatch ALL ready tickets before awaiting. Do NOT serialize independent tickets!

### Step 5: Active Pool Monitoring Loop (`factory_await_pool_worker`)
- **Continuous Traversal Directive**: You MUST maintain an active, autonomous execution loop. You are **strictly forbidden** from ending your turn and telling the user to re-run `continue-roadmap` when a batch finishes.
- Immediately enter the active monitoring loop using `factory_await_pool_worker(timeout_secs=600)`:
  1. **When a worker finishes (`event == "WORKER_FINISHED"`)**:
     - Check `ticket_id`, `worker_id`, `role`, and `status`.
     - When a Ticket Architect finishes (`role == "ARCHITECT"`):
       - If `status == "COMPLETED"`:
         - **Remote Synchronization**: Run `git push origin main` immediately so downstream tickets branch from up-to-date `main`.
         - Mark the ticket as `COMPLETED` in `openspec/roadmaps/<milestone-id>.md` (Section 2 & Section 5).
         - Dismiss the completed worker: `factory_dismiss_worker(ticket_id, worker_id)`.
         - **Evaluate Downstream Dependencies**: If any candidate tickets in `PLANNED` have all their dependencies satisfied and blast radii are disjoint, immediately call `factory_create_ticket` and `factory_spawn_worker(role="architect")` to launch them.
       - If `status == "FAILED"` or `ticket_state == "CONFLICT_NEEDS_ATTENTION"`:
         - Mark the ticket `CONFLICT_NEEDS_ATTENTION` in the roadmap manifest.
         - Escalate to the human user with exact failure details.
  2. **When the pool is drained (`event == "POOL_DRAINED"`)**:
     - Re-evaluate the entire roadmap manifest.
     - If all tickets are `COMPLETED`, proceed to Step 7.
     - If there are newly unblocked tickets in `PLANNED`, spawn them immediately and resume `factory_await_pool_worker`.
- **Repeat**: Continue calling `factory_await_pool_worker` until all roadmap tickets reach `COMPLETED` or a conflict halts execution. NEVER stop your turn to ask the user for another `continue-roadmap` invocation unless an unrecoverable system crash occurred.

### Step 6: Conflict & Circuit Breaker Handling
- If a ticket encounters a merge conflict (`CONFLICT_NEEDS_ATTENTION`) or exceeds review cycle limits (`MAX_REVIEW_CYCLES = 3`):
  - Mark it `CONFLICT_NEEDS_ATTENTION` in the roadmap manifest.
  - Pause dependent downstream tickets.
  - Keep independent parallel tracks running.
  - Escalate to the human user with exact details.

### Step 7: Milestone Conclusion
- When all tickets reach `COMPLETED`:
  - Inform the user:
    > *"🎉 All tickets in roadmap `<milestone-id>` have completed and merged into `main`! Issue `complete-roadmap <milestone-id>` to run end-to-end integration checks, archive proposals, and reconcile canonical specs."*
