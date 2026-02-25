---
name: security-audit
description: Security audit checklist for web applications and APIs. Use when reviewing code for security issues, hardening an application, or performing a security audit.
---

## Security Audit

### Audit Process

1. Map the attack surface (endpoints, inputs, data flows)
2. Run automated checks (see `scripts/check-secrets.sh`)
3. Walk through the OWASP checklist (see `references/OWASP_TOP10.md`)
4. Review authentication and authorization flows
5. Check dependency vulnerabilities
6. Document findings with severity ratings

### Quick Checks

- Search for hardcoded secrets, API keys, passwords
- Check that all user input is validated and sanitized
- Verify HTTPS is enforced everywhere
- Check CORS configuration
- Review error handling (no stack traces in production)
- Verify rate limiting on authentication endpoints

### Dependency Audit

```bash
# Rust
cargo audit
cargo deny check advisories

# Node.js
npm audit

# Python
pip-audit
safety check
```

### Severity Ratings

- **Critical**: Remote code execution, authentication bypass, data breach
- **High**: SQL injection, XSS, SSRF, privilege escalation
- **Medium**: Information disclosure, CSRF, insecure defaults
- **Low**: Missing headers, verbose errors, minor misconfigurations
