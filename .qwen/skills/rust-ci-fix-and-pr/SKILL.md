---
name: rust-ci-fix-and-pr
description: Fix Rust CI failures (cargo fmt + clippy) and create a clean PR from a dirty working tree
source: auto-skill
extracted_at: '2026-06-01T08:33:42.154Z'
---

# Fix Rust CI Failures and Create a Clean PR

## When to use

- `cargo fmt --check` or `cargo clippy -D warnings` fails in CI
- Working directory has other uncommitted changes unrelated to the fix
- Need to create a clean, focused PR on a separate branch

## Step 1 — Identify issues

Run the same commands CI runs:

```bash
cargo fmt --all -- --check 2>&1 | head -80
cargo clippy --all-targets -- -D warnings 2>&1 | tail -80
```

## Step 2 — Fix formatting

```bash
cargo fmt --all
```

This reformats all files in the workspace. Changes may span many files — this is expected since rustfmt enforces consistent style project-wide.

## Step 3 — Fix clippy lints

Common lints that appear when upgrading Rust toolchain versions:

| Lint | Fix |
|---|---|
| `manual_repeat_n` | `std::iter::repeat(x).take(n)` → `std::iter::repeat_n(x, n)` |
| `derivable_impls` | Add `#[derive(Default)]` to the struct, remove the manual `impl Default` block |
| `needless_borrow` | Remove unnecessary `&` references |
| `clone_on_copy` | Remove `.clone()` on `Copy` types |

For `derivable_impls`: only remove the manual impl if **all** fields themselves implement `Default` and the manual impl just delegates. If any field has a non-default value (e.g. `"admin123".into()`), keep the manual impl — clippy only flags it when all values match derived defaults (e.g. `String::new()`).

## Step 4 — Verify all CI checks pass locally

```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo check --all-targets
cargo test
```

All four must pass before committing.

## Step 5 — Create clean PR from dirty working tree

When the working branch has other uncommitted changes:

```bash
# 1. Stash all current work
git stash push -m "all-wip-changes"

# 2. Create fix branch from origin/master (clean base)
git checkout -b fix/ci-lint origin/master

# 3. Re-apply the fixes on this clean branch
#    (re-run cargo fmt, apply clippy fixes)

# 4. Verify CI passes on this branch too

# 5. Commit and push
git add -A
git commit -m "fix(ci): resolve cargo fmt and clippy failures"
git push -u origin fix/ci-lint

# 6. Create issue + PR via gh CLI
gh issue create --title "..." --body "..."
gh pr create --title "..." --body "..." --base master --head fix/ci-lint

# 7. Restore working tree
git checkout dev
git stash pop
```

## Step 6 — Diagnose Docker / GitHub Actions secrets failures

When Docker CI fails with `Error: Username and password required`, check where secrets are stored:

```bash
# Repository-level secrets (visible to all jobs)
gh api repos/{owner}/{repo}/actions/secrets

# Environment-level secrets (only visible to jobs declaring that environment)
gh api repos/{owner}/{repo}/environments/{name}/secrets
```

**Common pitfall:** Secrets configured under **Settings → Environments → production → Environment secrets** are invisible to workflow jobs unless the job declares `environment: production`. If you see this, add the environment to the job:

```yaml
jobs:
  docker:
    runs-on: ubuntu-latest
    environment: production   # ← required to access environment secrets
```

To make Docker Hub login gracefully optional (pass without secrets, e.g. for forks):

```yaml
- name: Check Docker Hub credentials
  id: dockerhub
  run: |
    if [ -n "${{ secrets.DOCKERHUB_USERNAME }}" ] && [ -n "${{ secrets.DOCKERHUB_TOKEN }}" ]; then
      echo "available=true" >> "$GITHUB_OUTPUT"
    else
      echo "available=false" >> "$GITHUB_OUTPUT"
    fi

- name: Log in to Docker Hub
  if: steps.dockerhub.outputs.available == 'true'
  uses: docker/login-action@v3
  with:
    username: ${{ secrets.DOCKERHUB_USERNAME }}
    password: ${{ secrets.DOCKERHUB_TOKEN }}
```

## Step 7 — Enrich GitHub repo profile

After initial CI is green, consider:

```bash
# Update description and add topic tags for discoverability
gh repo edit --description "..." \
  --add-topic rust --add-topic axum --add-topic docker ...

# Create first release
gh release create v0.3.0 --target master --title "v0.3.0 — ..." --notes "..."

# Add GHCR to Docker CI so Packages section shows a container image
# (uses built-in GITHUB_TOKEN, no extra secrets needed)
```

## Step 8 — Docker Hub collaborator push limitation

**Docker Hub free plan does NOT allow collaborators to push images**, even with a Read & Write Access Token. The `insufficient_scope: authorization failed` error occurs when the token belongs to a collaborator rather than the repo owner.

Diagnose with:
```
ERROR: failed to push davepaine/wechat-rs:latest: push access denied,
repository does not exist or may require authorization:
server message: insufficient_scope: authorization failed
```

Solution: create the Access Token under the **repo owner's** Docker Hub account (e.g. `davepaine`), not the collaborator's account.

## Step 9 — Rename a GitHub Environment

When renaming an environment (e.g. `production` → `docker-registry`):

```bash
# 1. Create new environment with branch policy
gh api repos/{owner}/{repo}/environments/{new-name} -X PUT \
  --input <(echo '{"deployment_branch_policy":{"protected_branches":false,"custom_branch_policies":true}}')

# 2. Add allowed branches
gh api repos/{owner}/{repo}/environments/{new-name}/deployment-branch-policies \
  -X POST -f name="master" -f type="branch"

# 3. Set secrets manually (gh secret copy between envs is not supported)
echo "username" | gh secret set SECRET_NAME --env {new-name}
echo "token"    | gh secret set SECRET_NAME --env {new-name}

# 4. Update workflow: environment: {old-name} → environment: {new-name}

# 5. After PR merges, delete old environment
gh api repos/{owner}/{repo}/environments/{old-name} -X DELETE
```

## Step 10 — Workaround for `gh pr edit` GraphQL error

`gh pr edit` may fail with:
```
GraphQL: Projects (classic) is being deprecated...
(repository.pullRequest.projectCards)
```

Workaround — use the REST API directly:
```bash
gh api repos/{owner}/{repo}/pulls/{N} -X PATCH \
  -f title="new title" -F body=@/tmp/body.md
```

## Notes

- Use `Closes #N` in the commit message body to auto-close the issue when the PR merges
- The commit message should list each fix category (fmt, clippy lint names) for clarity
- `cargo fmt` changes may touch files beyond the ones with CI failures — include them all since they are all rustfmt normalization
- Always stash → branch → fix → commit → push → PR → checkout back → stash pop when working from a dirty tree
- Docker Hub Access Token permissions: Read | Read & Write | Read & Delete | Read, Write & Delete — pushing requires at least **Read & Write**
