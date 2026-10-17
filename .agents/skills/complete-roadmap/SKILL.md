---
name: complete-roadmap
description: Concludes a multi-ticket roadmap. Validates end-to-end integration across merged tickets, archives change proposals, reconciles canonical specifications, and delivers the milestone acceptance report.
---

# `complete-roadmap` Workflow

Use this skill when all tickets in a roadmap have been implemented and merged, and the milestone is ready for final archival and delivery.

## Procedure

1. **Identify Target Roadmap**:
   - If specified (e.g., `complete-roadmap milestone-01`), target `openspec/roadmaps/<milestone-id>.md`.
   - If omitted, scan `openspec/roadmaps/` for any roadmap where all tickets are `COMPLETED`.
   - Verify every ticket in the `Ticket Dependency Matrix` has reached `COMPLETED` status.
   - If any ticket is still `PLANNED`, `IN_PROGRESS`, or `CONFLICT_NEEDS_ATTENTION`, halt and report remaining tickets to the user.

2. **End-to-End Milestone Validation**:
   - Execute the project-wide test suite, compiler checks, or integration test suite on `main` to verify overall system cohesion after all ticket merges.
   - Confirm all acceptance criteria in Section 5 ("Progress & Milestone Completion Criteria") of the roadmap manifest are satisfied.

3. **Archive Change Proposals**:
   - Ensure directory `openspec/changes/archive/` exists.
   - For each ticket in the roadmap:
     - Move `openspec/changes/<ticket-id>/` to `openspec/changes/archive/<ticket-id>/`.

4. **Reconcile Canonical Specifications**:
   - Update canonical domain specifications under `openspec/specs/` to document the new architectural capabilities, schemas, and features delivered across this milestone.
   - Ensure the canonical specifications reflect the merged, production state of the codebase.

5. **Archive Roadmap Manifest**:
   - In `openspec/roadmaps/<milestone-id>.md`:
     - Check off all items in Section 5 ("Progress & Milestone Completion Criteria").
     - Update YAML frontmatter with `completed_at: "<ISO-8601>"` and `status: "COMPLETED"`.
   - Optionally archive to `openspec/roadmaps/archive/<milestone-id>.md`.

6. **Push Milestone Delivery to Remote**:
   - Commit all specification updates and roadmap archival artifacts to `main`:
     `git add openspec/ && git commit -m "chore(milestone): complete and reconcile <milestone-id>"`
   - Push all changes to remote: `git push origin main`.
   - Verify that local `main` is completely synchronized with `origin/main`.

7. **Deliver Milestone Acceptance Report**:
   - Present a concise, structured acceptance report to the user:
     - **Milestone Summary**: Objective achieved and scope delivered.
     - **Tickets Completed**: List of all tickets, their thinking levels, and key deliverables.
     - **Integration Verification**: Test and validation summary across all deliverables.
     - **Canonical Specs Reconciled**: Files updated in `openspec/specs/`.
     - **Remote Sync Status**: Confirmed pushed to `origin/main`.
     - **Next Steps**: Recommend next roadmap milestone or invite user to initiate a new task.
