---
name: code-review
description: Structured code review methodology. Use when reviewing code changes, PRs, or diffs.
---

## Code Review Methodology

### Review Checklist

1. **Correctness**: Does the code do what it's supposed to?
2. **Security**: Any injection risks, exposed secrets, or unsafe operations?
3. **Performance**: Obvious bottlenecks, unnecessary allocations, N+1 queries?
4. **Readability**: Could another developer understand this in 6 months?
5. **Testing**: Are the changes tested? Are edge cases covered?
6. **API design**: Are public interfaces clean and well-documented?

### Review Process

1. Read the PR description and linked issues first
2. Look at the diff as a whole before line-by-line review
3. Start with the tests to understand intent
4. Review the implementation against the tests
5. Check for missing tests (error paths, edge cases, concurrency)

### Feedback Style

- Be specific: reference file and line
- Distinguish between blocking issues and suggestions
- Explain the "why" behind feedback
- Offer alternatives when pointing out problems
- Acknowledge good patterns and improvements
