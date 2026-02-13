# UX/UI Review Checklist

Applies to user-facing app flows, docs UX, and notebook UX.

## Flow and State

- User-visible state transitions are explicit and reversible where possible.
- Loading, success, empty, and error states are all represented.
- Critical actions provide clear confirmation and recovery path.

## Accessibility and Clarity

- Labels and interaction text are specific and concise.
- Keyboard-only operation is viable for primary actions.
- Color is not the only signal for status/alerts.

## Feedback and Responsiveness

- User receives immediate acknowledgment for initiated actions.
- Long operations expose progress or actionable status.
- Error messages include next-step guidance.

## Documentation and Notebook UX

- Docs examples are runnable and match current API/protocol behavior.
- Notebook cells have clear execution order and expected outputs.
- Experimental and production paths are clearly separated.
