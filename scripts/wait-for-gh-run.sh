#!/usr/bin/env bash
# Poll a GitHub Actions run until it completes, printing status updates.
#
# Usage examples:
#   scripts/wait-for-gh-run.sh --run 17901972778
#   scripts/wait-for-gh-run.sh --workflow Release --branch main
#   scripts/wait-for-gh-run.sh  # picks latest run on current branch
#
# Dependencies: gh (GitHub CLI), jq.

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: wait-for-gh-run.sh [OPTIONS]

Options:
  -r, --run ID           Run ID to monitor.
  -w, --workflow NAME    Workflow name or filename to pick the latest run.
  -b, --branch BRANCH    Branch to filter when selecting a run (default: current branch).
  -i, --interval SECONDS Polling interval in seconds (default: 8).
  -L, --failure-logs     Print logs for any job that does not finish successfully.
  -h, --help             Show this help message.

If neither --run nor --workflow is provided, the latest run on the current
branch is selected automatically.
EOF
}

require_binary() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: '$1' not found in PATH" >&2
    exit 1
  fi
}

RUN_ID=""
WORKFLOW=""
BRANCH=""
INTERVAL="8"
PRINT_FAILURE_LOGS=false
AUTO_SELECTED_RUN=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    -r|--run)
      RUN_ID="${2:-}"
      shift 2
      ;;
    -w|--workflow)
      WORKFLOW="${2:-}"
      shift 2
      ;;
    -b|--branch)
      BRANCH="${2:-}"
      shift 2
      ;;
    -i|--interval)
      INTERVAL="${2:-}"
      shift 2
      ;;
    -L|--failure-logs)
      PRINT_FAILURE_LOGS=true
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown option '$1'" >&2
      usage >&2
      exit 1
      ;;
  esac
done

require_binary gh
require_binary jq

default_branch() {
  local branch=""
  if command -v git >/dev/null 2>&1; then
    if branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null); then
      if [[ -n "$branch" && "$branch" != "HEAD" ]]; then
        echo "$branch"
        return 0
      fi
    fi

    if branch=$(git symbolic-ref --quiet --short refs/remotes/origin/HEAD 2>/dev/null); then
      branch="${branch#origin/}"
      if [[ -n "$branch" ]]; then
        echo "$branch"
        return 0
      fi
    fi

    if branch=$(git remote show origin 2>/dev/null | awk '/HEAD branch/ {print $NF}'); then
      if [[ -n "$branch" ]]; then
        echo "$branch"
        return 0
      fi
    fi
  fi

  echo "main"
}

select_latest_run() {
  local workflow="$1"
  local branch="$2"
  local json
  if ! json=$(gh run list --workflow "$workflow" --branch "$branch" --limit 1 --json databaseId,status,conclusion,displayTitle,workflowName,headBranch 2>/dev/null); then
    echo "error: failed to list runs for workflow '$workflow'" >&2
    exit 1
  fi

  if [[ $(jq 'length' <<<"$json") -eq 0 ]]; then
    echo "error: no runs found for workflow '$workflow' on branch '$branch'" >&2
    exit 1
  fi

  jq -r '.[0].databaseId' <<<"$json"
}

select_latest_run_any() {
  local branch="$1"
  local json
  if ! json=$(gh run list --branch "$branch" --limit 1 --json databaseId,workflowName,displayTitle,headBranch 2>/dev/null); then
    echo "error: failed to list runs on branch '$branch'" >&2
    exit 1
  fi

  if [[ $(jq 'length' <<<"$json") -eq 0 ]]; then
    echo "error: no runs found on branch '$branch'" >&2
    exit 1
  fi

  WORKFLOW=$(jq -r '.[0].workflowName // ""' <<<"$json")
  jq -r '.[0].databaseId' <<<"$json"
}

format_duration() {
  local total="$1"
  local hours=$((total / 3600))
  local minutes=$(((total % 3600) / 60))
  local seconds=$((total % 60))
  if [[ $hours -gt 0 ]]; then
    printf '%dh%02dm%02ds' "$hours" "$minutes" "$seconds"
  elif [[ $minutes -gt 0 ]]; then
    printf '%dm%02ds' "$minutes" "$seconds"
  else
    printf '%ds' "$seconds"
  fi
}

if [[ -z "$BRANCH" ]]; then
  BRANCH=$(default_branch)
fi

if [[ -z "$RUN_ID" ]]; then
  if [[ -n "$WORKFLOW" ]]; then
    RUN_ID=$(select_latest_run "$WORKFLOW" "$BRANCH")
    AUTO_SELECTED_RUN=true
  else
    RUN_ID=$(select_latest_run_any "$BRANCH")
    AUTO_SELECTED_RUN=true
  fi
fi

if [[ -z "$RUN_ID" ]]; then
  echo "error: unable to determine run ID" >&2
  exit 1
fi

echo "Waiting for GitHub Actions run $RUN_ID..." >&2
if [[ "$AUTO_SELECTED_RUN" == true ]]; then
  if [[ -z "$WORKFLOW" ]]; then
    echo "Auto-selected latest run on branch '$BRANCH'." >&2
  else
    echo "Auto-selected latest '$WORKFLOW' run on branch '$BRANCH'." >&2
  fi
elif [[ -n "$WORKFLOW" ]]; then
  echo "Using workflow '$WORKFLOW' on branch '$BRANCH'." >&2
fi

last_status=""
last_jobs_snapshot=""
last_progress_snapshot=""

while true; do
  json=""
  if ! json=$(gh run view "$RUN_ID" --json status,conclusion,displayTitle,workflowName,headBranch,url,startedAt,updatedAt,jobs 2>/dev/null); then
    echo "$(date '+%Y-%m-%d %H:%M:%S') failed to fetch run info; retrying in $INTERVAL s" >&2
    sleep "$INTERVAL"
    continue
  fi

  status=$(jq -r '.status' <<<"$json")
  conclusion=$(jq -r '.conclusion // ""' <<<"$json")
  workflow_name=$(jq -r '.workflowName // "(unknown workflow)"' <<<"$json")
  display_title=$(jq -r '.displayTitle // "(no title)"' <<<"$json")
  branch_name=$(jq -r '.headBranch // "(unknown branch)"' <<<"$json")
  run_url=$(jq -r '.url // ""' <<<"$json")

  if [[ "$status" != "$last_status" ]]; then
    echo "$(date '+%Y-%m-%d %H:%M:%S') [$workflow_name] $display_title on branch '$branch_name' -> status: $status${conclusion:+, conclusion: $conclusion}" >&2
    [[ -n "$run_url" ]] && echo "  $run_url" >&2
    last_status="$status"
  fi

  jobs_snapshot=$(jq -r '.jobs[]? | "\(.name // "(no name)")|\(.status)//\(.conclusion // "")"' <<<"$json" | sort)

  if [[ "$jobs_snapshot" != "$last_jobs_snapshot" ]]; then
    if [[ -n "$jobs_snapshot" ]]; then
      echo "$(date '+%Y-%m-%d %H:%M:%S') job summary:" >&2
      jq -r '.jobs[]? | "  - " + (.name // "(no name)") + ": " + (.status // "?") + (if .status == "completed" and .conclusion != null then " (" + .conclusion + ")" else "" end)' <<<"$json" >&2
    fi
    last_jobs_snapshot="$jobs_snapshot"
  fi

  total_jobs=$(jq -r '.jobs | length' <<<"$json")
  completed_jobs=$(jq -r '[.jobs[]? | select(.status == "completed")] | length' <<<"$json")
  in_progress_jobs=$(jq -r '[.jobs[]? | select(.status == "in_progress")] | length' <<<"$json")
  queued_jobs=$(jq -r '[.jobs[]? | select(.status == "queued")] | length' <<<"$json")
  progress_snapshot="$completed_jobs/$total_jobs/$in_progress_jobs/$queued_jobs"

  if [[ "$status" != "completed" && "$total_jobs" != "0" && "$progress_snapshot" != "$last_progress_snapshot" ]]; then
    echo "$(date '+%Y-%m-%d %H:%M:%S') progress: $completed_jobs/$total_jobs completed ($in_progress_jobs in_progress, $queued_jobs queued)" >&2
    last_progress_snapshot="$progress_snapshot"
  fi

  failing_jobs=$(jq -c '
    .jobs[]? | select(
      .status == "completed" and (.conclusion // "") != "" and
      ((.conclusion | ascii_downcase) as $c | $c != "success" and $c != "skipped" and $c != "neutral")
    )
  ' <<<"$json")

  if [[ -n "$failing_jobs" ]]; then
    echo "$(date '+%Y-%m-%d %H:%M:%S') detected failing job(s) while run status is '$status'; exiting early." >&2
    if [[ "$PRINT_FAILURE_LOGS" == true ]]; then
      if [[ "$status" != "completed" ]]; then
        echo "Run $RUN_ID is still $status; skipping log download for now." >&2
      else
        while IFS= read -r job_json; do
          [[ -z "$job_json" ]] && continue
          job_id=$(jq -r '.databaseId // ""' <<<"$job_json")
          job_name=$(jq -r '.name // "(no name)"' <<<"$job_json")
          job_conclusion=$(jq -r '.conclusion // "unknown"' <<<"$job_json")
          echo "--- Logs for job: $job_name (ID $job_id, conclusion: $job_conclusion) ---" >&2
          if [[ -n "$job_id" ]]; then
            if ! gh run view "$RUN_ID" --log --job "$job_id" 2>&1; then
              echo "(failed to fetch logs for job $job_id)" >&2
            fi
          else
            echo "(job has no databaseId; skipping log fetch)" >&2
          fi
          echo "--- End logs for job: $job_name ---" >&2
        done <<<"$failing_jobs"
      fi
    fi
    exit 1
  fi

  if [[ "$status" == "completed" ]]; then
    started_at=$(jq -r '.startedAt // ""' <<<"$json")
    updated_at=$(jq -r '.updatedAt // ""' <<<"$json")
    duration=""
    if [[ -n "$started_at" && -n "$updated_at" ]]; then
      start_epoch=$(date -d "$started_at" +%s 2>/dev/null || true)
      end_epoch=$(date -d "$updated_at" +%s 2>/dev/null || true)
      if [[ -n "$start_epoch" && -n "$end_epoch" && "$end_epoch" -ge "$start_epoch" ]]; then
        duration=$(format_duration $((end_epoch - start_epoch)))
      fi
    fi
    if [[ "$conclusion" == "success" ]]; then
      if [[ -n "$duration" ]]; then
        echo "Run $RUN_ID succeeded in $duration." >&2
      else
        echo "Run $RUN_ID succeeded." >&2
      fi
      exit 0
    else
      if [[ "$PRINT_FAILURE_LOGS" == true ]]; then
        echo "Collecting logs for failed jobs..." >&2
        jq -r '.jobs[]? | select((.conclusion // "") != "success") | "\(.databaseId)\t\(.name // "(no name)")"' <<<"$json" \
          | while IFS=$'\t' read -r job_id job_name; do
              [[ -z "$job_id" ]] && continue
              echo "--- Logs for job: $job_name (ID $job_id) ---" >&2
              if ! gh run view "$RUN_ID" --log --job "$job_id" 2>&1; then
                echo "(failed to fetch logs for job $job_id)" >&2
              fi
              echo "--- End logs for job: $job_name ---" >&2
            done
      fi
      if [[ -n "$duration" ]]; then
        echo "Run $RUN_ID finished with conclusion '$conclusion' in $duration." >&2
      else
        echo "Run $RUN_ID finished with conclusion '$conclusion'." >&2
      fi
      exit 1
    fi
  fi

  sleep "$INTERVAL"
done
