# OWASP Top 10 (2021) Checklist

## A01: Broken Access Control

- [ ] Deny by default (except public resources)
- [ ] Implement access control once, reuse everywhere
- [ ] Enforce record ownership (users can't view/edit others' data)
- [ ] Disable directory listing
- [ ] Log access control failures, alert on repeated failures
- [ ] Rate limit API and controller access
- [ ] Invalidate sessions on logout / JWT tokens expire

## A02: Cryptographic Failures

- [ ] Classify data by sensitivity level
- [ ] Don't store sensitive data unnecessarily
- [ ] Encrypt all sensitive data at rest
- [ ] Use strong, current algorithms (AES-256, RSA-2048+)
- [ ] Enforce HTTPS with HSTS
- [ ] Don't use deprecated hash functions (MD5, SHA1)
- [ ] Use authenticated encryption (not just encryption)

## A03: Injection

- [ ] Use parameterized queries / prepared statements
- [ ] Use positive server-side input validation
- [ ] Escape special characters for any remaining dynamic queries
- [ ] Use LIMIT and other SQL controls to prevent mass disclosure
- [ ] Validate and sanitize all user-supplied data

## A04: Insecure Design

- [ ] Establish secure development lifecycle
- [ ] Use threat modeling for critical flows
- [ ] Write unit and integration tests for security controls
- [ ] Separate tenant data at all tiers
- [ ] Limit resource consumption per user/service

## A05: Security Misconfiguration

- [ ] Repeatable hardening process for all environments
- [ ] Remove unused features, frameworks, components
- [ ] Review and update configurations regularly
- [ ] Segmented application architecture
- [ ] Send security directives to clients (security headers)

## A06: Vulnerable and Outdated Components

- [ ] Remove unused dependencies
- [ ] Inventory client and server-side component versions
- [ ] Monitor CVE databases continuously
- [ ] Obtain components from official sources over secure links
- [ ] Monitor for unmaintained libraries

## A07: Identification and Authentication Failures

- [ ] Implement multi-factor authentication where possible
- [ ] Don't ship with default credentials
- [ ] Implement weak password checks
- [ ] Align password policies with current guidelines (NIST 800-63b)
- [ ] Limit failed authentication attempts (rate limit + lockout)
- [ ] Use server-side secure session manager

## A08: Software and Data Integrity Failures

- [ ] Use digital signatures to verify software/data
- [ ] Use trusted repositories for libraries and dependencies
- [ ] Use a supply chain security tool (SBOM, Dependabot, etc.)
- [ ] Review process for code and configuration changes
- [ ] Ensure CI/CD pipeline has proper segregation and access control

## A09: Security Logging and Monitoring Failures

- [ ] Log all login, access control, and input validation failures
- [ ] Logs have enough context for forensic analysis
- [ ] Log format is consumable by log management solutions
- [ ] High-value transactions have audit trail with integrity controls
- [ ] Establish effective monitoring and alerting

## A10: Server-Side Request Forgery (SSRF)

- [ ] Sanitize and validate all client-supplied input data
- [ ] Enforce URL schema, port, and destination with allowlist
- [ ] Don't send raw responses to clients
- [ ] Disable HTTP redirections
- [ ] Use network-level segmentation
