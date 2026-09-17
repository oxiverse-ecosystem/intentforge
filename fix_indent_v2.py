#!/usr/bin/env python3
"""Fix indentation in the constraint_fix_tests mod - dedent 8 spaces from lines that are over-indented."""
import sys

path = 'services/gateway/src/main.rs'
with open(path, 'r') as f:
    lines = f.readlines()

# The branch fix incorrectly left function bodies at 16-space indent.
# Functions are at 4-space indent (mod is at 0), so bodies must be at 8 spaces.
# 
# Lines to fix: from 17869 to 17905 (0-indexed: 17868-17904)
# These lines have 16 or 20 spaces of indent that should be 8 or 12.

fixed = 0
for i in range(17868, 17905):  # 0-indexed
    line = lines[i]
    stripped = line.lstrip(' ')
    n_spaces = len(line) - len(stripped)
    
    if n_spaces >= 16:
        # Dedent by 8 spaces
        lines[i] = ' ' * (n_spaces - 8) + stripped
        fixed += 1

print(f"Fixed {fixed} lines")

# Verify
print("\nVerification:")
for i in range(17865, 17910):
    line = lines[i]
    stripped = line.lstrip(' ')
    n_spaces = len(line) - len(stripped)
    if stripped:
        print(f"  {i+1}: {n_spaces} spaces | {line.rstrip()}")

with open(path, 'w') as f:
    f.writelines(lines)
print("\nFile written.")
