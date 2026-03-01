#!/usr/bin/env bash
# Scan for potential hardcoded secrets in the codebase
set -euo pipefail

echo "Scanning for potential secrets..."

# Common patterns
PATTERNS=(
    'password\s*=\s*["\x27][^"\x27]+'
    'api[_-]?key\s*=\s*["\x27][^"\x27]+'
    'secret\s*=\s*["\x27][^"\x27]+'
    'token\s*=\s*["\x27][^"\x27]+'
    'AWS_ACCESS_KEY_ID'
    'PRIVATE[_-]KEY'
    'BEGIN RSA PRIVATE KEY'
    'ghp_[a-zA-Z0-9]{36}'
    'sk-[a-zA-Z0-9]{48}'
)

FOUND=0

for pattern in "${PATTERNS[@]}"; do
    if grep -rn --include='*.rs' --include='*.py' --include='*.js' \
         --include='*.ts' --include='*.go' --include='*.java' \
         --include='*.yaml' --include='*.yml' --include='*.toml' \
         --include='*.json' --include='*.env' \
         -iE "$pattern" . 2>/dev/null; then
        FOUND=1
    fi
done

if [ "$FOUND" -eq 0 ]; then
    echo "No obvious secrets found."
else
    echo ""
    echo "WARNING: Potential secrets detected above. Review each match."
fi
