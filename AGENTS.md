# Repository Guidelines

## Project Structure & Module Organization
- `crates/`: Rust workspace crates — `server` (API + bins), `db` (SQLx models/migrations), `executors`, `services`, `utils`, `deployment`, `local-deployment`, `remote`.
- `agent-service/`: TypeScript Agent Service using Anthropic Agent SDK for interactive Claude sessions. Runs on port 3001.
- `frontend/`: React + TypeScript app (Vite, Tailwind). Source in `frontend/src`.
- `frontend/src/components/dialogs`: Dialog components for the frontend.
- `remote-frontend/`: Remote deployment frontend.
- `shared/`: Generated TypeScript types (`shared/types.ts`). Do not edit directly.
- `assets/`, `dev_assets_seed/`, `dev_assets/`: Packaged and local dev assets.
- `npx-cli/`: Files published to the npm CLI package.
- `scripts/`: Dev helpers (ports, DB preparation).
- `docs/`: Documentation files.

## Agent Service Architecture (Required)
The `agent-service/` provides interactive Claude agent sessions using the Anthropic Agent SDK:
- **Purpose**: Exclusive execution method for Claude Code - no subprocess fallback
- **Key features**: Pause/resume for user questions, session persistence, SSE streaming
- **Port**: Fixed at 3001 (configurable via `AGENT_SERVICE_PORT`)
- **Auth**: Trusts internal calls from Rust backend (Rust is the auth gateway)

### System Requirements
**The application requires Claude Code to be installed and agent-service to be running.**
- On startup, the app checks system readiness via `/api/system-readiness`
- If Claude Code is not installed or agent-service is unavailable, a blocking error dialog is shown
- Users cannot proceed until requirements are met
- No subprocess fallback exists - agent-service is mandatory

### Executor Model
The app exclusively uses **Claude Code** (`BaseCodingAgent.CLAUDE_CODE`):
- All other executor types (AMP, Codex, Copilot, Cursor, etc.) have been removed
- No model/variant selection UI exists - always uses Claude Code defaults
- The `execution_processes.executor` column always stores "CLAUDE_CODE"

### Core Services
- `src/services/agent-client.ts`: SDK wrapper with pause/resume logic
- `src/services/job-manager.ts`: Job lifecycle management (queued → running → paused → completed)
- `src/services/stream-manager.ts`: SSE event broadcasting
- `src/services/agent-registry.ts`: Agent type registry (coding, planning, testing, review, explore)
- `src/services/orchestration.ts`: Multi-phase workflow execution with autofix loops
- `src/services/dev-server.ts`: Dev server process management with port detection

### API Routes
- `POST /jobs` - Start a new job
- `GET /jobs/:id` - Get job status
- `POST /jobs/:id/resume` - Resume paused job with user input
- `POST /jobs/:id/cancel` - Cancel running job
- `GET /jobs/:id/stream` - SSE stream for job events
- `GET /workflows` - List available workflows
- `POST /workflows/execute` - Execute a workflow
- `GET /agent-types` - List agent types
- `POST /dev-servers` - Start a dev server
- `GET /dev-servers/running` - List running dev servers

### Rust Integration
- `crates/local-deployment/src/agent_service.rs` provides HTTP client (`AgentServiceClient`)
- `crates/local-deployment/src/container.rs` routes all execution through agent-service
- Session IDs emitted via `LogMsg::SessionId` for persistence
- Integrates with existing `CodingAgentTurn` model for session continuity

### Running Agent Service
Agent service must be running for the app to function:
```bash
cd agent-service && npm install && npm run dev
```

### Frontend Integration
- `frontend/src/components/dialogs/agent/AgentQuestionDialog.tsx` - Interactive question dialog
- `frontend/src/components/dialogs/global/SystemRequirementsErrorDialog.tsx` - Blocking error if requirements not met
- `frontend/src/hooks/useSystemReadiness.ts` - Hook for checking system requirements

## Managing Shared Types Between Rust and TypeScript

ts-rs allows you to derive TypeScript types from Rust structs/enums. By annotating your Rust types with #[derive(TS)] and related macros, ts-rs will generate .ts declaration files for those types.
When making changes to the types, you can regenerate them using `pnpm run generate-types`
Do not manually edit shared/types.ts, instead edit crates/server/src/bin/generate_types.rs

## Build, Test, and Development Commands
- Install: `pnpm i`
- Run dev (frontend + backend with ports auto-assigned): `pnpm run dev`
- Backend (watch): `pnpm run backend:dev:watch`
- Frontend (dev): `pnpm run frontend:dev`
- Type checks: `pnpm run check` (frontend) and `pnpm run backend:check` (Rust cargo check)
- Rust tests: `cargo test --workspace`
- Generate TS types from Rust: `pnpm run generate-types` (or `generate-types:check` in CI)
- Prepare SQLx (offline): `pnpm run prepare-db`
- Prepare SQLx (remote package, postgres): `pnpm run remote:prepare-db`
- Local NPX build: `pnpm run build:npx` then `pnpm pack` in `npx-cli/`

## Coding Style & Naming Conventions
- Rust: `rustfmt` enforced (`rustfmt.toml`); group imports by crate; snake_case modules, PascalCase types.
- TypeScript/React: ESLint + Prettier (2 spaces, single quotes, 80 cols). PascalCase components, camelCase vars/functions, kebab-case file names where practical.
- Keep functions small, add `Debug`/`Serialize`/`Deserialize` where useful.

## Testing Guidelines
- Rust: prefer unit tests alongside code (`#[cfg(test)]`), run `cargo test --workspace`. Add tests for new logic and edge cases.
- Frontend: ensure `pnpm run check` and `pnpm run lint` pass. If adding runtime logic, include lightweight tests (e.g., Vitest) in the same directory.

## Security & Config Tips
- Use `.env` for local overrides; never commit secrets. Key envs: `FRONTEND_PORT`, `BACKEND_PORT`, `HOST` 
- Dev ports and assets are managed by `scripts/setup-dev-environment.js`.
