---
name: start-task
description: Initiates a new software factory task. Engages the Architect in inception dialogue and drafts the OpenSpec specification.
---

# `start-task` Workflow

Use this skill when the human user initiates a new task, ticket, or feature request.

## Procedure

1. **Ensure Live Board is Open**:
   - Check if `factory-board` is already running (`pgrep -x factory-board`).
   - If not running, ensure it is opened in the bottom IDE terminal pane (`./target/release/factory-board`).

2. **Assign Ticket ID**:
   - Determine the next sequential or user-provided `ticket_id` (e.g., `ticket-001`).
   - Create the directories:
     - `openspec/changes/<ticket-id>/`
     - `.factory/tasks/<ticket-id>/briefs/`
     - `.factory/tasks/<ticket-id>/reports/`
     - `.factory/tasks/<ticket-id>/reviews/`

3. **Inception Dialogue**:
   - Clarify the user's requirements, expected outcomes, scope boundaries, and non-functional requirements.
   - Propose architectural approach and trade-offs.

4. **Draft OpenSpec Specification**:
   - Write `openspec/changes/<ticket-id>/spec.md` using `specs/templates/task-spec.template.md`.
   - Ensure explicit:
     - Requirements list
     - Verification criteria
     - Target architecture & affected modules
     - Max review cycles (default: 3)

5. **Initialize Ticket via MCP**:
   - Call MCP tool `factory_create_ticket(ticket_id, title, base_branch)`.
   - The MCP server sets up the ticket branch `factory/<ticket-id>/main`.

6. **Human Alignment Check**:
   - Summarize the drafted specification and brief decomposition to the user.
   - Inform the user: *"Specification drafted in `openspec/changes/<ticket-id>/spec.md` (state: PLANNING). When you are satisfied with this specification, issue `implement-task` to begin autonomous execution."*

7. **MANDATORY INCEPTION STOP GATE (DO NOT PROCEED)**:
   - **HALT EXECUTION IMMEDIATELY**: End your turn right now. Do not call any further tools.
   - You are **STRICTLY FORBIDDEN** from modifying application source files in this turn.
   - You are **STRICTLY FORBIDDEN** from spawning implementer workers or executing briefs in this turn.
   - You are **STRICTLY FORBIDDEN** from checking out `main`, running `git commit`, `git merge`, or `git push origin main`.
   - The specification MUST remain in `state: "PLANNING"` until the user explicitly issues `implement-task` or gives explicit approval.
   - Await the user's explicit response.
