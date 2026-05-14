# Tasks

| # | Status | Title | Files | Depends on | Capability | Spec row | Owner |
| - | - | - | - | - | - | - | - |
0|available|Split ContextMenu into rust/fleet-ui/src/overlay/context_menu.rs|`rust/fleet-ui/src/overlay/context_menu.rs`<br>`rust/fleet-ui/src/overlay.rs`<br>`rust/fleet-ui/tests/overlay_context_menu.rs`|-|rust_refactor|-|-
1|available|Split Spotlight into rust/fleet-ui/src/overlay/spotlight.rs|`rust/fleet-ui/src/overlay/spotlight.rs`<br>`rust/fleet-ui/tests/overlay_spotlight.rs`|-|rust_refactor|-|-
2|available|Split ActionSheet into rust/fleet-ui/src/overlay/action_sheet.rs|`rust/fleet-ui/src/overlay/action_sheet.rs`<br>`rust/fleet-ui/tests/overlay_action_sheet.rs`|-|rust_refactor|-|-
3|available|Split SessionSwitcher into rust/fleet-ui/src/overlay/session_switcher.rs|`rust/fleet-ui/src/overlay/session_switcher.rs`<br>`rust/fleet-ui/tests/overlay_session_switcher.rs`|-|rust_refactor|-|-
4|available|Cleanup overlay.rs — verify mod tree + remove any residual widget bodies|`rust/fleet-ui/src/overlay.rs`|0, 1, 2, 3|rust_refactor|-|-
