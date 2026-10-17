---
name: complete-task
description: Concludes a factory task. Delivers the acceptance report to the user, merges the ticket branch upon approval, and cleans up worktrees.
---

# `complete-task` Workflow

Use this skill when the user requests final delivery and completion of a verified task.

## Procedure

1. **Smart Ticket Resolution**:
   - If target ticket ID is explicitly provided (e.g. `complete-task ticket-001`), target that ticket.
   - If no ID is provided, query tickets via `factory_get_ticket_status`:
     - If exactly 1 ticket is in `READY_FOR_ACCEPTANCE`, auto-select it.
     - If multiple tickets are in `READY_FOR_ACCEPTANCE`, list them and prompt user to select which ticket to complete.
     - If 0 tickets are in `READY_FOR_ACCEPTANCE`, report current progress of in-flight tickets.

2. **Complete Ticket & Merge via MCP**:
   - Call `factory_complete_ticket(ticket_id, "main")`.
   - The MCP server merges `factory/<ticket-id>/main` into `main`, prunes worker worktrees, and updates ticket status in SQLite to `COMPLETED`. It does not push.

3. **OpenSpec Archival & Canonical Reconciliation**:
   - Move/archive `openspec/changes/<ticket-id>/` into `openspec/changes/archive/<ticket-id>/`.
   - Reconcile and update canonical system specifications under `openspec/specs/` to reflect the newly delivered capabilities.

4. **Push to Remote (SuperArchitect Responsibility)**:
   - Commit the archived changes and updated canonical specs:
     `git add openspec/ && git commit -m "chore(spec): reconcile canonical specs and archive <ticket-id>"`
   - Push the merged code and spec commits: `git push origin main`.
   - Confirm `main` is in sync with `origin/main`.

