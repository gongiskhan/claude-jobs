---
active: true
iteration: 1
max_iterations: 8
completion_promise: "LOCAL_FOLDER_PROJECT_CREATED"
started_at: "2026-01-05T18:51:29Z"
---

FEATURE: Add local Git folder selection for project creation. Currently users can only select from remote Git repos, but many users have existing local Git projects they want to manage. Implementation: 1) Add a 'Local Folder' tab/option in the project creation dialog (check frontend/src/components/dialogs for existing patterns). 2) Implement folder picker that validates the selected path has a .git directory. 3) Create project entry pointing to the local path instead of cloning. 4) Ensure the local project integrates with existing worktree and coding agent functionality. 5) Add appropriate error handling if folder doesn't exist or isn't a git repo. Think step by step. After implementing, use Chrome extension to: open project creation dialog, select the local folder option, pick an existing local git repo, verify the project is created and appears in the kanban board with correct path. Output LOCAL_FOLDER_PROJECT_CREATED when a local folder project is successfully created and visible in the UI.
