#!/bin/bash
# Git pre-commit hook: security & hygiene checks before every commit.
# Checks: git email (noreply), secrets in staged files, .gitignore coverage.
#
# Exit 0 = allow commit, Exit 1 = block commit.

set -euo pipefail

ERRORS=()
WARNINGS=()

# --- 1. Git email check ---
EMAIL=$(git config user.email 2>/dev/null || echo "")
if [ -z "$EMAIL" ]; then
  ERRORS+=("Git email is not configured")
elif ! echo "$EMAIL" | grep -qi "noreply"; then
  ERRORS+=("Git email '$EMAIL' may expose personal info. Use a noreply address.")
fi

# --- 2. Secret scan on staged files ---
STAGED_FILES=$(git diff --cached --name-only 2>/dev/null || echo "")

if [ -n "$STAGED_FILES" ]; then
  # Filter out binary/non-scannable files
  SCANNABLE=$(echo "$STAGED_FILES" | grep -vE '\.(png|jpg|jpeg|gif|ico|wasm|vsix|parquet|lock)$' || true)

  if [ -n "$SCANNABLE" ]; then
    PATTERNS=(
      '(?i)(api[_-]?key|secret[_-]?key|access[_-]?token|private[_-]?key)\s*[:=]'
      '(?i)(password|passwd|pwd)\s*[:=]\s*['"'"'""][^'"'"'""]+['"'"'""]'
      '-----BEGIN (RSA|EC|DSA|OPENSSH) PRIVATE KEY-----'
      'ghp_[a-zA-Z0-9]{36}'
      'sk-[a-zA-Z0-9]{20,}'
      'AKIA[0-9A-Z]{16}'
    )

    for file in $SCANNABLE; do
      [ -f "$file" ] || continue
      for pattern in "${PATTERNS[@]}"; do
        MATCHES=$(grep -nP "$pattern" "$file" 2>/dev/null || true)
        if [ -n "$MATCHES" ]; then
          while IFS= read -r match; do
            ERRORS+=("Secret pattern in $file:$match")
          done <<< "$MATCHES"
        fi
      done
    done
  fi
fi

# --- 3. .gitignore coverage ---
if [ -f ".gitignore" ]; then
  grep -q '\.env' .gitignore || WARNINGS+=(".gitignore: missing .env* pattern")
  grep -qE '\*\.pem|\*\.key' .gitignore || WARNINGS+=(".gitignore: missing *.pem / *.key pattern")
else
  WARNINGS+=(".gitignore file not found")
fi

# --- Output ---
if [ ${#WARNINGS[@]} -gt 0 ]; then
  echo "=== Pre-commit warnings ==="
  for warn in "${WARNINGS[@]}"; do
    echo "  WARN: $warn"
  done
fi

if [ ${#ERRORS[@]} -gt 0 ]; then
  echo "=== Pre-commit check FAILED ==="
  for err in "${ERRORS[@]}"; do
    echo "  FAIL: $err"
  done
  echo ""
  echo "Fix the issues above before committing."
  exit 1
fi

exit 0
