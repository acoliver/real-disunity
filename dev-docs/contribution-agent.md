# Agent contribution procedure

[AGENTS.md](../AGENTS.md) sets mandatory rules. [CONTRIBUTING.md](../CONTRIBUTING.md)
explains the contributor path and review policy. Use this procedure to apply them
without duplicating subsystem contracts.

1. Read the assigned task and repository instructions. Check the current branch
   and working tree, preserve existing changes and stay in your assigned checkout.
   Inspect only references permitted by the source-use policy.
2. Follow the [coordination skill](../.llxprt/skills/disunity-coordination/SKILL.md):
   check claims, identify your session and claim a bounded feature. Respect
   acknowledged external claims. If GitHub writes are prohibited by the task,
   use its existing claim and report any coordination blocker without posting.
3. Read [architecture](architecture.md#ownership) and the
   [code standards](../.llxprt/skills/disunity-code-standards/SKILL.md). Define the
   observable outcome and smallest coherent PR milestone before implementation.
4. Follow the [workflow skill](../.llxprt/skills/disunity-workflow/SKILL.md) for
   branch discipline and scratch ownership. Write the behavior test, confirm its
   intended failure, implement in the existing owner and update affected docs.
5. Run the relevant [quality gates](quality-gates.md). For game milestones use
   [play verification](../.llxprt/skills/disunity-play-verification/SKILL.md) and
   report exactly what was observed. Rendering alone supports a rendering milestone.
6. Review the diff, source-use summary and [PR checklist](../.github/pull_request_template.md).
   When authorized, post the review-ready PR, monitor checks and answer reviews.
   Otherwise document the delivery blocker. Never merge or enable automatic merging.
7. Maintainer agents review incoming PRs daily and report actionable findings on
   the PR. Offer permission-based integration help. Andrew performs every final
   merge manually; waiting for his review does not require closing the PR.

Keep the verification record in the PR. Use a short reference and permission
summary; do not create manifests or session receipts. Clean disposable files and
processes you created, without touching other sessions' work or output.
