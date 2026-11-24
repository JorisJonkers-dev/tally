#!/usr/bin/env bash
# Audits the GitHub repository settings that cannot live in a file and applies the ones that are missing.
# Needs admin rights to apply; the audit alone needs only read access.
#
#   scripts/setup-repo.sh --audit [owner/repo]     report what is present and what is missing; changes nothing
#   scripts/setup-repo.sh [owner/repo]             apply what is missing (DRY_RUN=1 prints the calls instead)
#
# SHA_PINNING_REQUIRED=false turns the "pin every action by commit SHA" requirement off (default true); every
# action in this repository is pinned by SHA, and the organisation's reusable workflows are pinned from v0.50.3.
# Covers: merge methods and auto-merge, secret scanning and push protection, Dependabot alerts, private
# vulnerability reporting, CodeQL, the workflow token, which actions may run, approval for outside contributors'
# workflow runs, labels, and the branch and tag rulesets. Some of these are unavailable on a private repository
# without GitHub Advanced Security: a failed call is reported, not fatal.
set -euo pipefail

mode=apply
if [ "${1:-}" = "--audit" ]; then mode=audit; shift; fi
repo=${1:-JorisJonkers-dev/tally}
here=$(dirname "$(readlink -f "$0")")
dry=${DRY_RUN:-0}
pin=${SHA_PINNING_REQUIRED:-true}
missing=0

say() { printf '  %-62s %s\n' "$1" "$2"; }
call() { # call METHOD PATH [json]: a failure is reported, not fatal, so one unavailable feature does not stop the rest
  local method=$1 path=$2 body=${3:-}
  if [ "$dry" = 1 ]; then printf '     + gh api -X %s %s %s\n' "$method" "$path" "$body"; return 0; fi
  if [ -n "$body" ]; then
    printf '%s' "$body" | gh api -X "$method" "$path" --input - >/dev/null || echo "     !! $method $path failed (feature unavailable, or no admin rights)" >&2
  else
    gh api -X "$method" "$path" >/dev/null || echo "     !! $method $path failed" >&2
  fi
}
# ensure LABEL TEST_COMMAND APPLY_FUNCTION: runs the test; when it fails, counts it and (unless auditing) applies.
ensure() {
  local label=$1 test=$2 apply=$3
  if eval "$test" >/dev/null 2>&1; then say "$label" "ok"; return; fi
  missing=$((missing + 1))
  if [ "$mode" = audit ]; then say "$label" "MISSING"; else say "$label" "applying"; "$apply"; fi
}
get() { gh api "$@" 2>/dev/null; }

repo_json=$(get "repos/$repo") || { echo "cannot read $repo (gh auth?)" >&2; exit 1; }
rj() { jq -e "$1" <<<"$repo_json"; }

echo "Repository settings of $repo ($mode)"
a_merge() { call PATCH "repos/$repo" '{"allow_squash_merge":true,"allow_rebase_merge":false,"allow_merge_commit":false,"allow_auto_merge":true,"delete_branch_on_merge":true,"squash_merge_commit_title":"PR_TITLE","squash_merge_commit_message":"PR_BODY","has_wiki":false,"has_projects":false}'; }
ensure "squash only, auto-merge, branch deleted after merge" \
  "rj '.allow_squash_merge and (.allow_rebase_merge|not) and (.allow_merge_commit|not) and .allow_auto_merge and .delete_branch_on_merge'" a_merge
a_scan() { call PATCH "repos/$repo" '{"security_and_analysis":{"secret_scanning":{"status":"enabled"},"secret_scanning_push_protection":{"status":"enabled"}}}'; }
ensure "secret scanning and push protection" \
  "rj '.security_and_analysis.secret_scanning.status==\"enabled\" and .security_and_analysis.secret_scanning_push_protection.status==\"enabled\"'" a_scan
a_alerts() { call PUT "repos/$repo/vulnerability-alerts"; }
ensure "Dependabot alerts (Renovate makes the update pull requests)" "gh api repos/$repo/vulnerability-alerts" a_alerts
a_pvr() { call PUT "repos/$repo/private-vulnerability-reporting"; }
ensure "private vulnerability reporting" "get repos/$repo/private-vulnerability-reporting | jq -e .enabled" a_pvr
a_codeql() { call PATCH "repos/$repo/code-scanning/default-setup" '{"state":"configured","query_suite":"default"}'; }
ensure "CodeQL default setup" "get repos/$repo/code-scanning/default-setup | jq -e '.state==\"configured\"'" a_codeql

echo "Actions"
a_token() { call PUT "repos/$repo/actions/permissions/workflow" '{"default_workflow_permissions":"read","can_approve_pull_request_reviews":false}'; }
ensure "workflow token read-only by default" "get repos/$repo/actions/permissions/workflow | jq -e '.default_workflow_permissions==\"read\"'" a_token
a_allowed() {
  call PUT "repos/$repo/actions/permissions" '{"enabled":true,"allowed_actions":"selected","sha_pinning_required":'"$pin"'}'
  call PUT "repos/$repo/actions/permissions/selected-actions" '{"github_owned_allowed":true,"verified_allowed":false,"patterns_allowed":["JorisJonkers-dev/*","jdx/mise-action@*","Swatinem/rust-cache@*","googleapis/release-please-action@*"]}'
}
ensure "only GitHub's, the organisation's and three named actions, all pinned by SHA" \
  "get repos/$repo/actions/permissions | jq -e --argjson want \"$pin\" '.allowed_actions==\"selected\" and .sha_pinning_required==\$want'" a_allowed
a_approval() { call PUT "repos/$repo/actions/permissions/fork-pr-contributor-approval" '{"approval_policy":"all_external_contributors"}'; }
ensure "outside contributors' workflow runs need approval" \
  "get repos/$repo/actions/permissions/fork-pr-contributor-approval | jq -e '.approval_policy==\"all_external_contributors\"'" a_approval

echo "Labels and rulesets"
a_labels() {
  local l name color desc
  for l in "allow-repository-hygiene-exception|BFD4F2|Bypasses the repository hygiene guard" \
           "dependencies|0366D6|Dependency updates" "type: chore|EDEDED|Chore" "component: dependencies|EDEDED|Dependencies"; do
    IFS='|' read -r name color desc <<<"$l"
    if [ "$dry" = 1 ]; then echo "     + gh label create '$name'"; else gh label create "$name" -R "$repo" --color "$color" --description "$desc" --force >/dev/null; fi
  done
}
ensure "labels (hygiene exception, Renovate's)" \
  "get repos/$repo/labels --paginate --jq '.[].name' | grep -qx allow-repository-hygiene-exception && get repos/$repo/labels --paginate --jq '.[].name' | grep -qx dependencies" a_labels
a_rulesets() { if [ "$dry" = 1 ]; then echo "     + $here/apply-ruleset.sh $repo"; else bash "$here/apply-ruleset.sh" "$repo"; fi; }
ensure "ruleset Main: Pipeline Complete, pull requests only, squash, linear history, no force push" \
  "get repos/$repo/rulesets | jq -e 'map(select(.name==\"Main\" and .enforcement==\"active\"))|length==1'" a_rulesets
ensure "ruleset Release tags: v* cannot be moved or deleted" \
  "get repos/$repo/rulesets | jq -e 'map(select(.name==\"Release tags\" and .enforcement==\"active\"))|length==1'" a_rulesets

echo "Who can change workflows (anyone with write access can read the repository's secrets)"
get "repos/$repo/collaborators?affiliation=all" --jq '.[] | select(.permissions.push or .permissions.admin) | "  \(.login): \(if .permissions.admin then "admin" else "write" end)"' || echo "  (cannot list: needs push access)"

echo
if [ "$mode" = audit ]; then
  echo "$missing item(s) missing. Run without --audit to apply them."
else
  echo "Applied what was missing ($missing). Run with --audit to confirm."
fi
cat <<NOTE

Still by hand:
  * Renovate (the organisation's app) onboards itself once renovate.json is on the default branch.
  * Create a release by merging the release-please pull request; nothing is published to a registry.
NOTE
