---
name: start-roadmap
description: Initiates a multi-ticket roadmap, analyzes file blast radius overlaps, establishes concurrency DAGs, and seeds OpenSpec change proposals.
---

# `start-roadmap` Workflow

Use this skill when initiating or decomposing large multi-ticket milestones, cross-cutting epics, or complex architectural initiatives across multiple sequential or parallel tickets.

## Procedure

1. **Inception Dialogue for High-Level Milestones / Epics**:
   - Engage with the user to clarify high-level milestone objectives, requirements, target deliverables, and architectural boundaries.
   - Determine the milestone identifier (e.g., `milestone-01` or descriptive name) and roadmap title.

2. **Decomposing Milestones into Bounded Tickets**:
   - Break down the milestone into fine-grained, cohesive tickets.
   - Assign cognitive sizing distribution to each ticket:
     - `LOW` [L]: Data types, asset loaders, constants, schemas, DTOs, fixtures, configs.
     - `MEDIUM` [M]: Discrete subsystems, services, components, isolated logic.
     - `HIGH` [H]: Core engine/game loops, coordination routines, collision physics, state machines.

3. **Blast Radius Analysis & Scope Collision Detection**:
   - Cross-reference file and subsystem paths touched by each proposed ticket.
   - Detect potential scope collisions: tickets targeting overlapping files or shared interfaces cannot be executed concurrently without risk of git merge conflicts.
   - Bound each ticket to discrete, disjoint paths where concurrency is desired.

4. **Tagging Concurrency Tracks & Dependency DAG**:
   - Analyze dependencies between tickets.
   - Tag each ticket's concurrency track:
     - `PARALLEL_ELIGIBLE`: Zero unmet dependencies, disjoint file blast radius. Eligible to spawn in parallel.
     - `BLOCKED_ON_<ticket>`: Dependent on artifacts or contracts from prior tickets. Serialized execution required.
   - Synthesize a Mermaid execution DAG graph representing ticket execution flow.

5. **Scaffolding the Roadmap Document**:
   - Create `openspec/roadmaps/<milestone-id>.md` using `specs/templates/roadmap.template.md`.
   - Populate:
     - YAML frontmatter (`roadmap_id`, `milestone`, `title`, `version`, `created_at`).
     - Section 1: Milestone Objective & Scope.
     - Section 2: Ticket Dependency Matrix.
     - Section 3: Blast Radius & Scope Collision Matrix.
     - Section 4: Execution DAG Diagram.
     - Section 5: Progress & Milestone Completion Criteria.

6. **Pre-Scaffolding Ticket Change Proposals**:
   - For each ticket identified in the roadmap, pre-scaffold its OpenSpec change proposal directory and draft:
     - Directory: `openspec/changes/<ticket-id>/`
     - Specification: `openspec/changes/<ticket-id>/spec.md` (using `specs/templates/task-spec.template.md`).
     - Ensure each ticket specification explicitly links back to the roadmap milestone and tracks dependencies.

7. **User Presentation & Execution Transition**:
   - Present the roadmap overview, DAG visualization, and ticket breakdown to the user.
   - Instruct the user: *"Roadmap scaffolding complete. When you are ready, issue `implement-roadmap` to autonomously execute the roadmap DAG across parallel tracks, or `implement-task <ticket-id>` to execute an individual ticket."*

8. **MANDATORY ROADMAP INCEPTION STOP GATE (DO NOT PROCEED)**:
   - **HALT EXECUTION IMMEDIATELY**: End your turn right now. Do not call any further tools.
   - You are **STRICTLY FORBIDDEN** from spawning Ticket Architects or leaf workers in this turn.
   - You are **STRICTLY FORBIDDEN** from modifying application source files in this turn.
   - You are **STRICTLY FORBIDDEN** from checking out `main`, running `git commit`, `git merge`, or `git push origin main`.
   - The roadmap and its constituent tickets MUST remain in `state: "PLANNING"` until the user explicitly issues `implement-roadmap` or `implement-task`.
   - Await the user's explicit response.
