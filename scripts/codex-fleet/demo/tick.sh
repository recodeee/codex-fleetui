#!/usr/bin/env bash
# Demo tick simulator. Mutates the synthetic plan + pane scrollback every
# few seconds so the dashboards animate. Run in the background by
# scripts/codex-fleet/demo/up.sh.
#
# State machine per task: available → claimed → in_progress → completed.
# Each tick: pick one ready task per idle agent, advance one in-progress
# task toward completion, refresh runtimes/headlines in the pane fixtures,
# rewrite the counters file.
set -euo pipefail

DEMO_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$DEMO_DIR/../../.." && pwd)"
PLAN_SLUG="demo-refactor-wave-2026-05-16"
# Runtime plan copy lives in openspec/plans/ (up.sh seeds it from the
# template under scripts/codex-fleet/demo/scenarios/). tick.sh mutates this
# copy in place; down.sh removes it.
PLAN_FILE="$REPO_ROOT/openspec/plans/$PLAN_SLUG/plan.json"
STATE_DIR="/tmp/claude-viz"
PANES_DIR="$STATE_DIR/demo-panes"
TICK_INTERVAL="${CODEX_FLEET_DEMO_TICK_INTERVAL:-3}"
LOOP_ON_DONE="${CODEX_FLEET_DEMO_LOOP:-1}"

AIDS=(magnolia sumac yarrow clover thistle fennel mallow borage)
EMOJIS=("●" "◐" "◑" "◒" "◓" "◔" "◕" "○")

trap 'echo "tick: stopped"; exit 0' INT TERM

is_demo_active() { [[ -f "$STATE_DIR/demo-active" ]]; }

reset_plan() {
    jq '(.tasks[]) |= (.status = "available"
                       | .claimed_by_agent = null
                       | .claimed_by_session_id = null
                       | .completed_summary = null)' \
        "$PLAN_FILE" > "$PLAN_FILE.tmp" && mv "$PLAN_FILE.tmp" "$PLAN_FILE"
}

deps_satisfied() {
    local idx="$1"
    local unmet
    unmet=$(jq --argjson idx "$idx" '
        .tasks[$idx].depends_on
        | map(. as $d | $d as $needle
              | ($needle | (. != null and . != ""))
              | if . then $needle else empty end)
        | map(. as $d | select(
              ($d | type == "number") and
              ([(input | .tasks[] | select(.subtask_index == $d) | .status)] | first != "completed")
          ))
        | length
    ' "$PLAN_FILE" "$PLAN_FILE" 2>/dev/null || echo 1)
    [[ "$unmet" == "0" ]]
}

# Find next available task whose deps are met. Returns subtask_index or empty.
next_ready_task() {
    local n
    n=$(jq '.tasks | length' "$PLAN_FILE")
    local i
    for ((i=0; i<n; i++)); do
        local status
        status=$(jq -r --argjson i "$i" '.tasks[$i].status' "$PLAN_FILE")
        if [[ "$status" != "available" ]]; then continue; fi
        # check deps
        local unmet
        unmet=$(jq --argjson i "$i" '
            .tasks[$i].depends_on as $deps
            | [$deps[] as $d | .tasks[] | select(.subtask_index == $d) | select(.status != "completed")]
            | length
        ' "$PLAN_FILE")
        if [[ "$unmet" == "0" ]]; then
            echo "$i"
            return
        fi
    done
}

agents_idle() {
    # An agent is "idle" if they have no in_progress task assigned in the plan.
    local aid
    for aid in "${AIDS[@]}"; do
        local active
        active=$(jq -r --arg aid "codex-$aid" '
            [.tasks[] | select(.claimed_by_agent == $aid and (.status == "claimed" or .status == "in_progress"))]
            | length
        ' "$PLAN_FILE")
        if [[ "$active" == "0" ]]; then
            echo "$aid"
        fi
    done
}

assign_task() {
    local idx="$1" aid="$2"
    jq --argjson idx "$idx" --arg aid "codex-$aid" \
        '(.tasks[] | select(.subtask_index == $idx) | .status) = "claimed"
       | (.tasks[] | select(.subtask_index == $idx) | .claimed_by_agent) = $aid
       | (.tasks[] | select(.subtask_index == $idx) | .claimed_by_session_id) = "demo-session-\($aid)"' \
        "$PLAN_FILE" > "$PLAN_FILE.tmp" && mv "$PLAN_FILE.tmp" "$PLAN_FILE"
}

advance_task() {
    # Move one randomly-picked claimed/in_progress task to the next state.
    local idx="$1" next_status="$2"
    jq --argjson idx "$idx" --arg s "$next_status" \
        '(.tasks[] | select(.subtask_index == $idx) | .status) = $s' \
        "$PLAN_FILE" > "$PLAN_FILE.tmp" && mv "$PLAN_FILE.tmp" "$PLAN_FILE"
}

complete_task() {
    local idx="$1"
    jq --argjson idx "$idx" \
        '(.tasks[] | select(.subtask_index == $idx) | .status) = "completed"
       | (.tasks[] | select(.subtask_index == $idx) | .completed_summary) =
           "Demo: synthetic completion at \(now | strftime("%H:%M:%S"))"' \
        "$PLAN_FILE" > "$PLAN_FILE.tmp" && mv "$PLAN_FILE.tmp" "$PLAN_FILE"
}

write_pane_fixture() {
    local aid="$1" idx="$2" runtime_s="$3"
    local title
    title=$(jq -r --argjson idx "$idx" '.tasks[] | select(.subtask_index == $idx) | .title' "$PLAN_FILE")
    local minutes=$((runtime_s / 60))
    local seconds=$((runtime_s % 60))
    cat > "$PANES_DIR/$aid.txt" <<EOF
codex 0.42.0 — admin-${aid}@example.dev
Connected. Boot complete.

> demo-refactor-wave-2026-05-16 / subtask $idx
  ${title}

gpt-5.5 high
Working (${minutes}m ${seconds}s)
EOF
}

write_pane_idle() {
    local aid="$1"
    cat > "$PANES_DIR/$aid.txt" <<EOF
codex 0.42.0 — admin-${aid}@example.dev
Connected. Boot complete.

› idle — polling Colony for next ready task...
EOF
}

write_pane_capped() {
    local aid="$1"
    cat > "$PANES_DIR/$aid.txt" <<EOF
codex 0.42.0 — admin-${aid}@example.dev
Connected. Boot complete.

› ⚠ hit your usage limit (5h cap). Pausing until reset.
EOF
}

write_counters() {
    local total in_prog blocked done_ ready
    total=$(jq '.tasks | length' "$PLAN_FILE")
    ready=$(jq '[.tasks[] | select(.status == "available")] | length' "$PLAN_FILE")
    in_prog=$(jq '[.tasks[] | select(.status == "in_progress" or .status == "claimed")] | length' "$PLAN_FILE")
    done_=$(jq '[.tasks[] | select(.status == "completed")] | length' "$PLAN_FILE")
    blocked=$((total - ready - in_prog - done_))
    jq -n \
        --argjson overview 8 \
        --argjson fleet 8 \
        --argjson plan "$total" \
        --argjson waves "$in_prog" \
        --argjson review "$done_" \
        --argjson ts "$(date +%s)" \
        '{overview:$overview, fleet:$fleet, plan:$plan, waves:$waves, review:$review, updated_at:$ts}' \
        > "$STATE_DIR/fleet-tab-counters.json"
}

# Track per-task elapsed runtime in seconds since claim.
declare -A task_runtime

tick_once() {
    # 1. Hand out tasks to idle agents.
    local aid
    while IFS= read -r aid; do
        [[ -z "$aid" ]] && continue
        local idx
        idx=$(next_ready_task)
        if [[ -n "$idx" ]]; then
            assign_task "$idx" "$aid"
            task_runtime[$idx]=0
        else
            # No ready task — show idle scrollback unless this aid is "capped"
            if [[ "$aid" == "clover" ]]; then
                write_pane_capped "$aid"
            else
                write_pane_idle "$aid"
            fi
        fi
    done < <(agents_idle)

    # 2. Advance every claimed/in_progress task by one tick.
    local rows
    rows=$(jq -c '.tasks[] | select(.status == "claimed" or .status == "in_progress") | {idx:.subtask_index, aid:.claimed_by_agent, status:.status}' "$PLAN_FILE")
    while IFS= read -r row; do
        [[ -z "$row" ]] && continue
        local idx status aid
        idx=$(echo "$row" | jq -r '.idx')
        status=$(echo "$row" | jq -r '.status')
        aid=$(echo "$row" | jq -r '.aid' | sed 's/^codex-//')
        local rt="${task_runtime[$idx]:-0}"
        rt=$((rt + TICK_INTERVAL))
        task_runtime[$idx]=$rt
        write_pane_fixture "$aid" "$idx" "$rt"

        if [[ "$status" == "claimed" && "$rt" -ge 4 ]]; then
            advance_task "$idx" "in_progress"
        elif [[ "$status" == "in_progress" && "$rt" -ge 18 ]]; then
            complete_task "$idx"
            unset 'task_runtime[$idx]'
        fi
    done <<<"$rows"

    write_counters
}

main() {
    # Wait briefly for up.sh to finish writing initial state.
    sleep 1
    reset_plan

    while is_demo_active; do
        tick_once

        # Loop scenario: if all tasks done, reset and start over.
        local done_count total
        done_count=$(jq '[.tasks[] | select(.status == "completed")] | length' "$PLAN_FILE")
        total=$(jq '.tasks | length' "$PLAN_FILE")
        if [[ "$done_count" == "$total" ]]; then
            if [[ "$LOOP_ON_DONE" == "1" ]]; then
                sleep 4
                reset_plan
                task_runtime=()
            else
                echo "tick: all tasks complete, exiting (CODEX_FLEET_DEMO_LOOP=0)"
                exit 0
            fi
        fi

        sleep "$TICK_INTERVAL"
    done
}

main "$@"
