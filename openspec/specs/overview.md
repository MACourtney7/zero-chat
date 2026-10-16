# System Architecture: zero_chat

## 1. Project Overview
`zero_chat` is a distributed or client-server chat application structured into discrete client and network modules.

## 2. Directory Layout
- `zero_client/`: Client-side application interface and UI / CLI frontend logic.
- `zero_network/`: Networking protocols, socket connections, packet handling, and server synchronization.
- `.agents/`: Antigravity IDE configuration and Agentic Software Factory plugin/MCP registration.
- `openspec/`: OpenSpec Spec-Driven Development specifications and change proposals.
- `factory.yaml`: Automation build/test/lint configurations for factory workers.

## 3. SDD Workflow
All feature development proceeds through the OpenSpec SDD workflow:
- Specifications reside in `openspec/specs/` and active proposals in `openspec/changes/`.
- Work is coordinated via the Agentic Software Factory with isolated worktrees.
