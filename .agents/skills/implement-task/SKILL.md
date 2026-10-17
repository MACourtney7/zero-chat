---
name: implement-task
description: Launches autonomous factory execution. The Architect breaks down the task into cognitive-sized briefs, spawns parallel worktree subagents, validates, and conducts review.
---

# `implement-task` Workflow

Use this skill when the user triggers execution for a prepared ticket.

## Procedure

1. **Ensure Live Board is Active**:
   - Verify that `factory-board` is running (`pgrep -x factory-board`).
   - If not running, launch it in the terminal panel so all worker activity is visible in real-time (`./target/release/factory-board` or `factory-board`).

2. **Verify Task Specification**:
   - Verify task specification exists at `openspec/changes/<ticket-id>/spec.md` with finalized requirements and acceptance criteria.
   - Transition ticket state to `IN_PROGRESS` if not already updated.

3. **Decompose into Granular, Parallel Briefs**:
   - Break the work down into tightly bounded briefs in `.factory/tasks/<ticket-id>/briefs/` (1 to 3 files max per brief).
   - **Anti-Monolith Mandate**: Never bundle disparate domains into one worker (e.g. world + audio + particles). Split them into separate briefs (`brief-audio.md`, `brief-particles.md`, `brief-world.md`).
   - Assign cognitive sizing:
     - `LOW`: Boilerplate, sound triggers, asset loaders, DTOs, fixtures.
     - `MEDIUM`: Individual subsystems, particle emitters, UI components.
     - `HIGH`: Core loop, collision physics, algorithms, concurrency.

4. **Batch-Spawn Parallel Workers via MCP**:
   - **Mandatory Concurrency**: Spawn ALL independent briefs in the current stage concurrently BEFORE awaiting returns:
     ```text
     // Spawn all parallel briefs first:
     factory_spawn_worker(ticket_id, role="implementer", brief_path="brief-audio.md", thinking_level="LOW")
     factory_spawn_worker(ticket_id, role="implementer", brief_path="brief-particles.md", thinking_level="MEDIUM")
     factory_spawn_worker(ticket_id, role="implementer", brief_path="brief-world.md", thinking_level="MEDIUM")
     ```
   - Each worker runs in its own isolated Git worktree; all will appear on the live board with spinning pipes simultaneously.
   - **CRITICAL RESTRICTION**: You MUST NOT touch files inside `.factory/worktrees/` or write worker reports yourself. Do NOT attempt to implement the brief in your chat. Immediately proceed to Step 5 (`factory_await_next_worker`).

5. **Drain Return Queue**:
   - Cycle through worker completion:
     ```text
     while remaining_active_workers > 0:
         worker = factory_await_next_worker(ticket_id)
         inspect worker.report_path
         if worker.status == "SUCCESS":
             factory_merge_worker(ticket_id, worker.id)
         else:
             handle_failure(worker)
         factory_dismiss_worker(ticket_id, worker.id)
     ```
   - Never abandon spawned workers.

6. **Validation Stage**:
   - Write validation brief `.factory/tasks/<ticket-id>/briefs/brief-validation.md`.
   - Call `factory_spawn_worker(ticket_id, role="validator", brief_path, thinking_level="LOW")`.
   - Await completion using `factory_await_next_worker(ticket_id)`, inspect validation report, and dismiss the validator with `factory_dismiss_worker(ticket_id, worker.id)`.

7. **Review Stage (Review Loop & Circuit Breaker)**:
   - For cycle 1 to `max_review_cycles` (default 3):
     - Write review brief `.factory/tasks/<ticket-id>/briefs/brief-review-<cycle>.md`.
     - Call `factory_spawn_worker(ticket_id, role="reviewer", brief_path, thinking_level="HIGH")`.
     - Await completion using `factory_await_next_worker(ticket_id)` and dismiss reviewer with `factory_dismiss_worker(ticket_id, worker.id)`.
     - IF verdict is `APPROVE`: Proceed to Step 8.
     - IF verdict is `REVISE`:
       - Spawn targeted implementer to remediate findings.
       - Re-validate.
     - IF cycle limit exceeded:
       - **Trip Circuit Breaker**: Halt autonomous loops and prompt the user for guidance.

8. **Stage for Acceptance**:
   - Notify the user that implementation, validation, and review are complete.
   - Instruct the user:
     > *"Implementation, validation, and code review complete with `APPROVE` verdict. Issue `/complete-task <ticket-id>` to review the final acceptance report, merge to `main`, archive the change proposal, and push to remote."*

