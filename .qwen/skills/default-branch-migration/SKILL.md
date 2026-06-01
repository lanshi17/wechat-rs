---
name: default-branch-migration
description: Migrate repository default branch and update all CI/environment configurations
source: auto-skill
extracted_at: '2026-06-01T09:38:30.428Z'
---

# Migrating Default Branch

When changing the default branch (e.g., from `master` to `main` or `dev`), multiple configurations need updating beyond just the branch pointer.

## Step 1: Change the default branch

```bash
gh repo edit --default-branch <new-branch>
```

## Step 2: Update CI workflow triggers

Check all `.github/workflows/*.yml` files for branch references:

```yaml
# Before
on:
  push:
    branches: [master]
  pull_request:
    branches: [master]

# After
on:
  push:
    branches: [dev]
  pull_request:
    branches: [dev]
```

**Watch for:**
- `push` triggers
- `pull_request` base branches
- `workflow_dispatch` with branch filters
- Path filters with branch conditions

## Step 3: Update Environment deployment branch policies

Environments can restrict which branches can deploy:

```bash
# List current policies
gh api repos/{owner}/{repo}/environments/{env}/deployment-branch-policies

# Delete old policy
gh api repos/{owner}/{repo}/environments/{env}/deployment-branch-policies/{policy-id} -X DELETE

# Add new policy
gh api repos/{owner}/{repo}/environments/{env}/deployment-branch-policies -X POST \
  -f name="<new-branch>" -f type="branch"
```

## Step 4: Update branch protection rules

```bash
# Check existing protections
gh api repos/{owner}/{repo}/branches/{old-branch}/protection

# Remove old protections (if any)
gh api repos/{owner}/{repo}/branches/{old-branch}/protection -X DELETE

# Add protections to new branch
gh api repos/{owner}/{repo}/branches/{new-branch}/protection -X PUT \
  --input protection-rules.json
```

## Step 5: Handle merge conflicts

When merging the old default branch into the new one, conflicts may arise:

```bash
git checkout <new-branch>
git merge origin/<old-branch>

# Resolve conflicts, keeping the intended behavior
git add .
git commit
```

## Step 6: Clean up

After merging, delete the old branch if no longer needed:

```bash
git push origin --delete <old-branch>
git branch -D <old-branch>
```

## Checklist

- [ ] Default branch changed via `gh repo edit`
- [ ] All workflow triggers updated (push, PR, release, etc.)
- [ ] Environment deployment branch policies updated
- [ ] Branch protection rules migrated
- [ ] Merge conflicts resolved
- [ ] Old branch deleted (if applicable)
- [ ] Documentation updated (README, CONTRIBUTING, etc.)

## Common scenarios

**master → main**: Standard GitHub recommendation  
**master → dev**: Development-focused workflow (PRs target dev, master for releases)  
**main → develop**: GitFlow-style branching
