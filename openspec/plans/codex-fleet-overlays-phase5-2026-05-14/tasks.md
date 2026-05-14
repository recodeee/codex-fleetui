# Tasks

| # | Status | Title | Files | Depends on | Capability | Spec row | Owner |
| - | - | - | - | - | - | - | - |
0|available|fleet-ui::overlay::ContextMenu widget — port from POC|`rust/fleet-ui/src/overlay.rs`<br>`rust/fleet-ui/tests/overlay_context_menu.rs`<br>`rust/fleet-ui/src/lib.rs`|-|ui_work|-|-
1|available|fleet-ui::overlay::Spotlight widget — port from POC with interactive state|`rust/fleet-ui/src/overlay.rs`<br>`rust/fleet-ui/tests/overlay_spotlight.rs`<br>`rust/fleet-ui/src/lib.rs`|0|ui_work|-|-
2|available|fleet-ui::overlay::ActionSheet + SessionSwitcher widgets — port from POC|`rust/fleet-ui/src/overlay.rs`<br>`rust/fleet-ui/tests/overlay_action_sheet.rs`<br>`rust/fleet-ui/tests/overlay_session_switcher.rs`<br>`rust/fleet-ui/src/lib.rs`|1|ui_work|-|-
3|available|Wire Spotlight + ContextMenu keybindings into fleet-watcher|`rust/fleet-watcher/src/main.rs`<br>`rust/fleet-watcher/Cargo.toml`|1, 2|ui_work|-|-
4|available|Wire same keybindings into fleet-state + fleet-plan-tree + fleet-waves|`rust/fleet-state/src/main.rs`<br>`rust/fleet-plan-tree/src/main.rs`<br>`rust/fleet-waves/src/main.rs`<br>`rust/fleet-ui/src/overlay.rs`|3|ui_work|-|-
5|available|Soak test all four binaries against the live fleet + tick the openspec checkboxes|`openspec/changes/fleet-tui-ratatui-port-2026-05-14/tasks.md`|4|test_work|-|-
