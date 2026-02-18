# Advanced Workbench Contract (VSCode-Inspired)

## Scope

This contract applies to `UiMode::Advanced` in `neurohid-hub`.
`UiMode::Standard` behavior remains unchanged.

## Shell Regions

1. Primary sidebar (left): single-column shell with top lane strip, lane-scoped navigation,
   and bottom-pinned settings anchor.
2. Center panel: active screen/editor surface.
3. Bottom panel: tabbed utility surface (`Problems`, `Runtime`, `Logs`, `Console`).
4. Status bar: low-noise runtime summary + deterministic panel actions.

## Context Lanes

1. `Ops`: `Dashboard`, `Devices`, `Profiles`, `Calibration`
2. `Analysis`: `Visualization`
3. `Labs`: `Python Lab`, `Jupyter IDE`
4. `Config`: `Settings`

## Interaction Contract

1. Status bar right actions switch/toggle bottom tabs in fixed order:
   `Problems`, `Runtime`, `Logs`, `Console`.
2. Runtime triage content is first-class in the bottom panel.
3. Detached runtime log window is replaced in Advanced mode by bottom `Logs` tab.
4. Stream console can render embedded in bottom `Console` tab.
5. Command palette (`Ctrl+Shift+P`) dispatches common workbench and runtime actions.
6. Advanced mode does not render a separate activity rail panel.

## Keyboard Navigation

1. `Ctrl+Shift+P`: open command palette
2. `Ctrl+Shift+O/A/L/C`: switch activity lanes
3. `Ctrl+Shift+S`: focus/open sidebar navigation context
4. `Ctrl+Shift+ArrowUp/Down`: move previous/next entry in current sidebar context
5. `Ctrl+J`: toggle bottom panel visibility
6. `Alt+Left/Right`: cycle bottom panel tabs
7. `Ctrl+B`: toggle sidebar collapsed/open
8. In command palette: `ArrowUp/ArrowDown` selects entries, `Enter` executes selection

## Acceptance Focus

1. Advanced mode visually reads as an IDE workbench.
2. Live Ops triage does not require modal windows.
3. Cross-surface navigation is low-friction from status, problems, and command palette.

## Workflow SLA

1. From runtime warning signal to actionable triage control should take no more than
   3 user interactions in Advanced mode.
2. Detailed runtime troubleshooting should be completable from bottom `Runtime`/`Problems`
   tabs without mandatory screen hopping.
