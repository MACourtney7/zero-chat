---
name: implement-task
description: Launches autonomous factory execution. The Architect breaks down the task into cognitive-sized briefs, spawns parallel worktree subagents, validates, and conducts review.
---

# `implement-task` Workflow

Use this skill when the user triggers execution for a prepared ticket.

## Procedure

1. **Ensure Live Board is Active**:
   - Verify that `factory-board` is running (`pgrep -x factory-board`).
   - If not running, launch it in the terminal panel so all worker activity is visible in real-time.

2. **Verify Task Specification**:
   - Verify task specification exists at `openspec/changes/<ticket-id>/spec.md`.

3. **Spawn Detached Autonomous Architect**:
   - Call MCP tool to spawn the autonomous Architect worker:
     ```text
     factory_spawn_worker(
         ticket_id="<ticket-id>",
         role="architect",
         brief_path="openspec/changes/<ticket-id>/spec.md",
         thinking_level="HIGH"
     )
     ```

4. **Detach & Inform User**:
   - Immediately detach and inform the user:
     > *"Ticket `<ticket-id>` detached to autonomous Architect. The live board will reflect worker progress. With default auto-complete enabled, verified code will merge to `main` upon review approval, and I will push it to the remote."*

5. **Push on Completion (SuperArchitect Responsibility)**:
   - When `factory_get_ticket_status` shows the ticket as `COMPLETED` (auto-merged into `main` by the Ticket Architect), run `git push origin main`.
   - The Ticket Architect never pushes; if the ticket ends in `CONFLICT_NEEDS_ATTENTION`, do not push and escalate to the user.

