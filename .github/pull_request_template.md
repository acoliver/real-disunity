## Scope and behavior

Link the issue or acknowledged claim. Describe the outcome, acceptance criteria
and remaining gaps. For a game milestone, state whether it renders, accepts input
or is playable; a whole game port is not required.

## Verification

List commands actually run, results and blocked prerequisites. Include the public
reproduction path and concise evidence appropriate to this milestone.

## References and permissions

Briefly summarize references used and the permission basis for game material
inspected or shared, or state that no game material was used. Do not attach
restricted content or provenance manifests. Follow
[CONTRIBUTING.md](https://github.com/acoliver/real-disunity/blob/main/CONTRIBUTING.md#permitted-references-and-inspection).

## Checklist

Use this checklist to identify remaining work before review:

- [ ] The change extends existing owners and adds no per-game overlays or parallel engine.
- [ ] Original game scripts, assets and behavior remain unchanged apart from API binding and harness configuration, if applicable.
- [ ] Source use follows the policy, including no Unity documentation or implementation inspection.
- [ ] Behavioral tests and relevant public CPU, managed and GPU checks are reported, or the documentation-only scope is stated.
- [ ] Tests and quality limits remain intact; fork CI needs no secrets or private checkout.
- [ ] Third-party additions have an MIT-compatible license and required notices, if applicable.
- [ ] Affected README pages, user documentation and subsystem contracts are current.
- [ ] The PR is ready for review or its blocker is documented. All seven hosted checks are required for protected main.

Andrew intends human review daily; maintainer agents also review daily. This does
not guarantee a same-day merge. Only Andrew manually merges all PRs. Agents never
merge, enable auto-merge or use a merge queue. Agent-authored PRs identify the
session as described in [AGENTS.md](https://github.com/acoliver/real-disunity/blob/main/AGENTS.md#coordination-and-review-authority).
