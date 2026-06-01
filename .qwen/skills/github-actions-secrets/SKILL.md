---
name: github-actions-secrets
description: Diagnose why GitHub Actions secrets are not accessible in workflows
source: auto-skill
extracted_at: '2026-06-01T09:38:30.428Z'
---

# Diagnosing GitHub Actions Secrets Access

When a workflow fails because it can't access secrets (e.g., "Username and password required"), follow this diagnostic flow.

## Step 1: Identify where secrets are stored

```bash
# Check Repository-level secrets
gh api repos/{owner}/{repo}/actions/secrets

# Check Environment-level secrets
gh api repos/{owner}/{repo}/environments/{env-name}/secrets
```

**Key distinction:**
- **Repository secrets**: Available to all jobs automatically
- **Environment secrets**: Only available when the job declares `environment: <name>`

## Step 2: Check if the workflow declares the environment

If secrets are under an Environment, the workflow job **must** include:

```yaml
jobs:
  deploy:
    runs-on: ubuntu-latest
    environment: my-environment  # ← Required to access env secrets
```

Without this declaration, the job cannot see Environment secrets, even if they exist.

## Step 3: Check deployment branch policies

Environments can restrict which branches can deploy to them:

```bash
gh api repos/{owner}/{repo}/environments/{env-name}/deployment-branch-policies
```

If `custom_branch_policies: true`, only explicitly allowed branches can access the environment. Ensure your branch is in the allow list.

## Common Pitfalls

### Docker Hub specifics

- **Token permissions**: Access Tokens have levels (Read, Read & Write, etc.). Pushing images requires **Read & Write** minimum.
- **Collaborator push**: Docker Hub **free plan does NOT allow collaborators to push images**. Only the repository owner can push. Being a "collaborator" on Docker Hub is read-only on free plans.
- **Workaround**: Use a token created under the Docker Hub account that owns the repository.

### GHCR (GitHub Container Registry)

- Uses built-in `GITHUB_TOKEN`, no extra secrets needed
- Requires `permissions: packages: write` in the workflow
- Image name: `ghcr.io/${{ github.repository }}`

## Resolution checklist

1. [ ] Secrets exist at the correct level (Repository vs Environment)
2. [ ] Workflow job declares `environment:` if using Environment secrets
3. [ ] Branch policy allows the deploying branch
4. [ ] Token has sufficient permissions (Read & Write for push)
5. [ ] For Docker Hub: token belongs to the repo owner, not a collaborator
