---
name: docker-workflow
description: Docker containerization best practices and multi-stage build patterns. Use when writing Dockerfiles, docker-compose configs, or containerizing applications.
---

## Docker Workflow

### Dockerfile Best Practices

- Use multi-stage builds to minimize final image size
- Pin base image versions (never use `latest` in production)
- Order layers from least to most frequently changing
- Combine RUN commands to reduce layers
- Use `.dockerignore` to exclude build context bloat
- Run as non-root user in production images
- Use COPY instead of ADD unless extracting archives

### Multi-Stage Pattern

See `references/MULTI_STAGE.md` for language-specific multi-stage templates.

### Security

- Scan images with `docker scout` or `trivy`
- Never store secrets in image layers
- Use `--no-cache-dir` for pip, `--no-cache` for apk
- Pin package versions in RUN commands

### Docker Compose

See `references/COMPOSE_PATTERNS.md` for common service composition patterns.

### Health Checks

Always define HEALTHCHECK for production services:

```dockerfile
HEALTHCHECK --interval=30s --timeout=3s --retries=3 \
  CMD curl -f http://localhost:8080/health || exit 1
```
