---
name: api-design
description: REST and gRPC API design guidelines with OpenAPI patterns. Use when designing, implementing, or reviewing HTTP APIs or gRPC services.
---

## API Design Guidelines

### REST Conventions

- Use nouns for resources, not verbs: `/users`, not `/getUsers`
- Use plural nouns: `/users/123`, not `/user/123`
- Use HTTP methods correctly: GET (read), POST (create), PUT (replace),
  PATCH (partial update), DELETE (remove)
- Use HTTP status codes correctly (see `references/STATUS_CODES.md`)
- Version in URL path: `/api/v1/users`
- Use kebab-case for URLs, camelCase for JSON fields

### Pagination

Always paginate list endpoints:

```json
{
  "data": [...],
  "pagination": {
    "total": 142,
    "page": 1,
    "per_page": 20,
    "next_cursor": "abc123"
  }
}
```

Prefer cursor-based over offset-based for large datasets.

### Error Responses

Consistent error format across all endpoints:

```json
{
  "error": {
    "code": "VALIDATION_ERROR",
    "message": "Invalid email format",
    "details": [
      {"field": "email", "issue": "Must be a valid email address"}
    ]
  }
}
```

### OpenAPI

See `assets/openapi-template.yml` for a starter OpenAPI spec.
Document every endpoint, parameter, and response schema.

### Rate Limiting

- Return `429 Too Many Requests` with `Retry-After` header
- Include rate limit headers: `X-RateLimit-Limit`, `X-RateLimit-Remaining`
- Use sliding window algorithm, not fixed window
