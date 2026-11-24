#!/usr/bin/env bash
# Apply every ruleset in .github/rulesets/*.json to a repository: create it if the name is absent,
# update it in place otherwise. Idempotent. Needs admin rights on the repository.
# Usage: scripts/apply-ruleset.sh <owner/repo>   (default: the current repository)
set -euo pipefail

repo="${1:-$(gh repo view --json nameWithOwner -q .nameWithOwner)}"
dir="$(dirname "$0")/../.github/rulesets"

for file in "$dir"/*.json; do
  name="$(jq -r .name "$file")"
  id="$(gh api "/repos/$repo/rulesets" --jq ".[] | select(.name==\"$name\") | .id" 2>/dev/null || true)"
  if [[ -n $id ]]; then
    echo "Updating ruleset '$name' ($id) on $repo"
    gh api -X PUT "/repos/$repo/rulesets/$id" --input "$file" >/dev/null
  else
    echo "Creating ruleset '$name' on $repo"
    gh api -X POST "/repos/$repo/rulesets" --input "$file" >/dev/null
  fi
done
echo "Done. 'Pipeline Complete' is the required check on the default branch of $repo."
