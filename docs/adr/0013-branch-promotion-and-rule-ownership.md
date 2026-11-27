# ADR-0013: Branch promotion and ownership of repository rules
- Status: Accepted
- Date: 2026-11-27
- Decision owners: Product architecture (engine, platform and VS Code maintainers)
- Repository scope: cross-repository
- Supersedes: none. It refines how `main` is updated, from "Repository operations" in [ADR-0009](0009-multi-repository-architecture.md).
- Superseded by: none

## Context

Each repository has two long-lived branches, `development` and `main`. Both are protected by rulesets that require pull requests, passing checks and linear history, and they allow only squash or rebase merges (ADR-0009, FND-01).

On 2026-11-27, engine PR #5 (ENG-01) was opened against `main`, which is the default base for new pull requests, instead of `development`. The squash merge turned every commit `main` lacked, FND-01 through ENG-01, into one new commit. `main` and `development` then had identical files but separate histories, so the next `development` → `main` pull request would have conflicted on almost every file. A revert (PR #6) did not help: it was cut from `main`, so merged into `development` it was an empty commit. The repair was a one-off forced update of `main` to `development`'s commit, with the protection briefly disabled.

Two gaps caused this:

1. Nothing stops a pull request into `main` from coming from a branch other than `development`. GitHub rulesets control who may change a branch and which checks must pass, but have no rule for the source branch.
2. Even a correct `development` → `main` pull request is merged by squash or rebase. Both create new commits that `development` does not have, so the histories split on every promotion.

The files that define the repository rules (CI workflows, boundary checks, dependency matrices, formatter settings, contracts, golden fixtures) also need named owners, so that a contributor's pull request cannot quietly weaken them once the team grows.

## Problem

How does `main` receive work so that it always shares `development`'s history, how is a pull request from any other branch blocked, and who must review changes to the rules themselves?

## Decision

1. **`development` is the integration branch.** All work reaches it through pull requests, as before.
2. **`main` only moves forward to a commit of `development`.** A maintainer on the `protect-main` bypass list updates it with a fast-forward push, for example `git push origin origin/development:refs/heads/main`. No merge commit, squash or rebase is involved, so `main` and `development` keep one history. A forced update of `main` is never routine. If history has split, it is repaired only after showing that the two trees are identical, with `--force-with-lease` set to the expected commit.
3. **Pull requests into `main` must come from this repository's `development`.** Each repository has `.github/workflows/branch-policy.yml` with the job `source-branch`. It runs only for pull requests into `main`, and fails unless the source branch is `development` and the source repository is the same repository (forks fail). `source-branch` is a required status check in `protect-main` only. It is not required in `protect-development`, where it never runs.
4. **Rule files have code owners.** Each repository has `.github/CODEOWNERS`, which lists the paths that define its rules and nothing else, so ordinary changes need no extra review. While the project has a single maintainer, the owner is that maintainer's account, GitHub only requests a review, and nothing is blocked (GitHub does not let authors approve their own pull requests). When the repositories move to an organization with a second maintainer, the owner becomes a core team, and both rulesets turn on "Require review from Code Owners" with at least one approval.
5. **The default branch of each repository is `development`**, so new pull requests target it unless someone changes the base on purpose.

## Alternatives considered

- **Squash or rebase pull requests from `development` into `main`.** These are the only merge methods the linear-history rulesets allow. Every promotion would create commits missing from `development` and split the histories. Rejected.
- **Merge commits into `main`.** These would keep the histories joined, but break the linear-history rule and add a merge commit to `main` on every promotion. Rejected.
- **A single long-lived branch (`main` only).** This is simpler, but loses the separation between integrated work and promoted work that the project uses. Rejected for now; it can be revisited.
- **An external CI server such as Jenkins, which keeps pipeline definitions outside the repository.** External CI only reports a status, and GitHub's rulesets still decide whether a merge happens, so this adds a server to operate without adding protection. Rejected.
- **Centrally required workflows from a separate repository (organization rulesets).** Contributors could not edit these at all. They need an organization and possibly a paid plan. Deferred until the repositories move to an organization.
- **`pull_request_target` for the branch-policy workflow,** so it always runs the base branch's copy. This removes the ability of a source branch to edit the check, but runs with base-branch permissions on untrusted pull requests. The check exists to stop mistakes, and code owners cover deliberate edits to `.github/`. Rejected.

## Consequences

- Positive: `main` always equals a reviewed, CI-green commit of `development`, with the same history. A wrong-base pull request into `main` shows a failing required check and cannot be merged. Changes to rule files are routed to their owners.
- Negative: promotion needs a maintainer on the bypass list, which is a privileged permission. `source-branch` stops mistakes, not maintainers or bypass actors who choose to skip checks. A pull request can edit the workflow to make it pass, and until code-owner review is required, only review catches that.
- Mitigation: keep the bypass list to the maintainers who promote. Turn on code-owner review as soon as there is a second maintainer. Changes to `.github/` are owned, so edits to the check itself are reviewed.

## Implementation and verification

- 2026-11-27: `.github/workflows/branch-policy.yml` (job `source-branch`) and `.github/CODEOWNERS` were added to all three repositories. The workflow file is identical in each. Its script was run locally against passing and failing cases. `development` from the same repository passes. A feature branch, a revert branch, `development` from a fork, an empty name, `Development`, `development ` with a trailing space, and two names carrying shell commands all fail, and neither command ran. A probe without the same-repository test let the fork case through. Every path in each `CODEOWNERS` exists and is tracked.
- Repository settings (made by the owner, not by files): `source-branch` required in `protect-main`; the promoting maintainer on the `protect-main` bypass list; default branch `development`.
- Engine PR #5 is the incident that led to this decision. `main` was realigned to `development` at `457cdfb` (ENG-01, PR #7), and the two branches have had one history since.

## Related records

- [ADR-0009](0009-multi-repository-architecture.md): repository boundaries and operations. This ADR refines how `main` is updated.
- Architecture Blueprint v1.3 §7: each repository owns its branch protection and code ownership rules.
- Traceable Implementation Plan v1.2 §5.1: ENG-01 (history note), FND-01, FND-05.
