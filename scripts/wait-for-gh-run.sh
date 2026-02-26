#!/usr/bin/env bash
# Poll a GitHub Actions run until it completes, printing status updates.
#
# Supports two backends:
#  - `gh` (preferred when authenticated)
#  - GitHub REST API via `curl` (automatic fallback for public repos or when gh auth is unavailable)
#
# Usage examples:
#   scripts/wait-for-gh-run.sh --run 17901972778
#   scripts/wait-for-gh-run.sh --workflow Release --branch main --repo just-every/code
#   scripts/wait-for-gh-run.sh  # picks latest run on current branch/repo

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: wait-for-gh-run.sh [OPTIONS]

Options:
  -r, --run ID           Run ID to monitor.
  -w, --workflow NAME    Workflow name or filename to pick the latest run.
  -b, --branch BRANCH    Branch to filter when selecting a run (default: current branch).
  -R, --repo OWNER/REPO  Repository to query (default: infer from git/GITHUB_REPOSITORY).
  -i, --interval SECONDS Polling interval in seconds (default: 8).
  -L, --failure-logs     Print logs for failed jobs when supported.
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
REPO=""
INTERVAL="8"
PRINT_FAILURE_LOGS=false
AUTO_SELECTED_RUN=false
BACKEND=""

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
    -R|--repo)
      REPO="${2:-}"
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

require_binary jq
require_binary curl

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

infer_repo_from_remote() {
  local url
  url=$(git remote get-url origin 2>/dev/null || true)
  if [[ -z "$url" ]]; then
    return 1
  fi

  case "$url" in
    git@github.com:*.git)
      echo "${url#git@github.com:}" | sed 's/\.git$//'
      return 0
      ;;
    git@github.com:*)
      echo "${url#git@github.com:}"
      return 0
      ;;
    https://github.com/*.git)
      echo "${url#https://github.com/}" | sed 's/\.git$//'
      return 0
      ;;
    https://github.com/*)
      echo "${url#https://github.com/}"
      return 0
      ;;
    ssh://git@github.com/*)
      echo "${url#ssh://git@github.com/}" | sed 's/\.git$//'
      return 0
      ;;
  esac

  return 1
}

resolve_repo() {
  if [[ -n "$REPO" ]]; then
    echo "$REPO"
    return 0
  fi

  if [[ -n "${GITHUB_REPOSITORY:-}" ]]; then
    echo "$GITHUB_REPOSITORY"
    return 0
  fi

  if command -v git >/dev/null 2>&1; then
    if repo=$(infer_repo_from_remote); then
      echo "$repo"
      return 0
    fi
  fi

  echo "error: unable to infer repository; pass --repo OWNER/REPO" >&2
  exit 1
}

api_headers() {
  local token="${GH_TOKEN:-${GITHUB_TOKEN:-}}"
  local headers=(
    -H "Accept: application/vnd.github+json"
    -H "X-GitHub-Api-Version: 2022-11-28"
  )
  if [[ -n "$token" ]]; then
    headers+=(-H "Authorization: Bearer $token")
  fi
  printf '%s\n' "${headers[@]}"
}

api_get() {
  local path="$1"
  local url="https://api.github.com${path}"
  local headers=()
  while IFS= read -r line; do
    headers+=("$line")
  done < <(api_headers)

  curl -fsSL "${headers[@]}" "$url"
}

is_integer() {
  [[ "$1" =~ ^[0-9]+$ ]]
}

resolve_workflow_id_api() {
  local workflow_input="$1"

  if is_integer "$workflow_input"; then
    echo "$workflow_input"
    return 0
  fi

  if [[ "$workflow_input" == *.yml || "$workflow_input" == *.yaml ]]; then
    echo "$workflow_input"
    return 0
  fi

  local workflows
  workflows=$(api_get "/repos/${REPO}/actions/workflows?per_page=100") || {
    echo "error: failed to list workflows via GitHub API" >&2
    exit 1
  }

  local matched
  matched=$(jq -r --arg name "$workflow_input" '
    .workflows[]? | select(.name == $name) | .id
  ' <<<"$workflows" | head -n1)

  if [[ -z "$matched" || "$matched" == "null" ]]; then
    echo "error: workflow '$workflow_input' not found in repo '$REPO'" >&2
    exit 1
  fi

  echo "$matched"
}

select_latest_run_gh() {
  local workflow="$1"
  local branch="$2"
  local json
  if ! json=$(gh run list --repo "$REPO" --workflow "$workflow" --branch "$branch" --limit 1 --json databaseId,status,conclusion,displayTitle,workflowName,headBranch 2>/dev/null); then
    echo "error: failed to list runs for workflow '$workflow'" >&2
    exit 1
  fi

  if [[ $(jq 'length' <<<"$json") -eq 0 ]]; then
    echo "error: no runs found for workflow '$workflow' on branch '$branch'" >&2
    exit 1
  fi

  jq -r '.[0].databaseId' <<<"$json"
}

select_latest_run_any_gh() {
  local branch="$1"
  local json
  if ! json=$(gh run list --repo "$REPO" --branch "$branch" --limit 1 --json databaseId,workflowName,displayTitle,headBranch 2>/dev/null); then
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

select_latest_run_api() {
  local workflow="$1"
  local branch="$2"
  local path

  if [[ -n "$workflow" ]]; then
    local workflow_id
    workflow_id=$(resolve_workflow_id_api "$workflow")
    path="/repos/${REPO}/actions/workflows/${workflow_id}/runs?branch=${branch}&per_page=1"
  else
    path="/repos/${REPO}/actions/runs?branch=${branch}&per_page=1"
  fi

  local json
  json=$(api_get "$path") || {
    echo "error: failed to list runs via GitHub API" >&2
    exit 1
  }

  local count
  count=$(jq '.workflow_runs | length' <<<"$json")
  if [[ "$count" -eq 0 ]]; then
    if [[ -n "$workflow" ]]; then
      echo "error: no runs found for workflow '$workflow' on branch '$branch'" >&2
    else
      echo "error: no runs found on branch '$branch'" >&2
    fi
    exit 1
  fi

  local run_id
  run_id=$(jq -r '.workflow_runs[0].id' <<<"$json")
  if [[ -z "$run_id" || "$run_id" == "null" ]]; then
    echo "error: unable to determine run ID from API response" >&2
    exit 1
  fi

  if [[ -z "$WORKFLOW" ]]; then
    WORKFLOW=$(jq -r '.workflow_runs[0].name // ""' <<<"$json")
  fi

  echo "$run_id"
}

fetch_run_snapshot_gh() {
  local run_id="$1"
  gh run view "$run_id" --repo "$REPO" --json status,conclusion,displayTitle,workflowName,headBranch,url,startedAt,updatedAt,jobs 2>/dev/null
}

fetch_run_snapshot_api() {
  local run_id="$1"
  local run_json
  local jobs_json

  run_json=$(api_get "/repos/${REPO}/actions/runs/${run_id}") || return 1
  jobs_json=$(api_get "/repos/${REPO}/actions/runs/${run_id}/jobs?per_page=100") || return 1

  jq -n \
    --argjson run "$run_json" \
    --argjson jobs "$jobs_json" \
    '{
      status: $run.status,
      conclusion: $run.conclusion,
      displayTitle: $run.display_title,
      workflowName: $run.name,
      headBranch: $run.head_branch,
      url: $run.html_url,
      startedAt: $run.run_started_at,
      updatedAt: $run.updated_at,
      jobs: [($jobs.jobs // [])[] | . + {databaseId: (.id|tostring)}]
    }'
}

print_api_failure_job_refs() {
  local json="$1"
  jq -r '
    .jobs[]?
    | select(
        .status == "completed" and
        (.conclusion // "") != "" and
        ((.conclusion | ascii_downcase) as $c | $c != "success" and $c != "skipped" and $c != "neutral")
      )
    | "  - " + (.name // "(no name)") + ": " + (.html_url // "(no url)")
  ' <<<"$json" >&2
}

determine_backend() {
  if command -v gh >/dev/null 2>&1; then
    if gh run list --repo "$REPO" --limit 1 --json databaseId >/dev/null 2>&1; then
      echo "gh"
      return 0
    fi
  fi

  echo "api"
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

REPO=$(resolve_repo)
BACKEND=$(determine_backend)

if [[ "$BACKEND" == "gh" ]]; then
  echo "Using GitHub CLI backend for run monitoring (repo: $REPO)." >&2
else
  echo "Using GitHub REST API fallback backend for run monitoring (repo: $REPO)." >&2
  echo "Reason: gh unavailable or unauthenticated for run queries." >&2
fi

if [[ -z "$RUN_ID" ]]; then
  if [[ "$BACKEND" == "gh" ]]; then
    if [[ -n "$WORKFLOW" ]]; then
      RUN_ID=$(select_latest_run_gh "$WORKFLOW" "$BRANCH")
      AUTO_SELECTED_RUN=true
    else
      RUN_ID=$(select_latest_run_any_gh "$BRANCH")
      AUTO_SELECTED_RUN=true
    fi
  else
    RUN_ID=$(select_latest_run_api "$WORKFLOW" "$BRANCH")
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
  if [[ "$BACKEND" == "gh" ]]; then
    if ! json=$(fetch_run_snapshot_gh "$RUN_ID"); then
      echo "$(date '+%Y-%m-%d %H:%M:%S') failed to fetch run info via gh; retrying in $INTERVAL s" >&2
      sleep "$INTERVAL"
      continue
    fi
  else
    if ! json=$(fetch_run_snapshot_api "$RUN_ID"); then
      echo "$(date '+%Y-%m-%d %H:%M:%S') failed to fetch run info via API; retrying in $INTERVAL s" >&2
      sleep "$INTERVAL"
      continue
    fi
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

  jobs_snapshot=$(jq -r '.jobs[]? | "\(.name // "(no name)")|\(.status // "")|\(.conclusion // "")"' <<<"$json" | sort)
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
      if [[ "$BACKEND" == "gh" ]]; then
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
              if ! gh run view "$RUN_ID" --repo "$REPO" --log --job "$job_id" 2>&1; then
                echo "(failed to fetch logs for job $job_id)" >&2
              fi
            else
              echo "(job has no databaseId; skipping log fetch)" >&2
            fi
            echo "--- End logs for job: $job_name ---" >&2
          done <<<"$failing_jobs"
        fi
      else
        echo "Failure logs are not downloaded in API fallback mode. Failed job URLs:" >&2
        print_api_failure_job_refs "$json"
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
    fi

    if [[ "$PRINT_FAILURE_LOGS" == true ]]; then
      if [[ "$BACKEND" == "gh" ]]; then
        echo "Collecting logs for failed jobs..." >&2
        jq -r '.jobs[]? | select((.conclusion // "") != "success") | "\(.databaseId)\t\(.name // "(no name)")"' <<<"$json" \
          | while IFS=$'\t' read -r job_id job_name; do
              [[ -z "$job_id" ]] && continue
              echo "--- Logs for job: $job_name (ID $job_id) ---" >&2
              if ! gh run view "$RUN_ID" --repo "$REPO" --log --job "$job_id" 2>&1; then
                echo "(failed to fetch logs for job $job_id)" >&2
              fi
              echo "--- End logs for job: $job_name ---" >&2
            done
      else
        echo "Failure logs are not downloaded in API fallback mode. Failed job URLs:" >&2
        print_api_failure_job_refs "$json"
      fi
    fi

    if [[ -n "$duration" ]]; then
      echo "Run $RUN_ID finished with conclusion '$conclusion' in $duration." >&2
    else
      echo "Run $RUN_ID finished with conclusion '$conclusion'." >&2
    fi
    exit 1
  fi

  sleep "$INTERVAL"
done

