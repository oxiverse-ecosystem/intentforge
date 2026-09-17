#!/usr/bin/env python3
"""Fix indentation in constraint_fix_tests mod - dedent function bodies from 16→8 and 20→8 spaces."""
import re

path = 'services/gateway/src/main.rs'
with open(path, 'r') as f:
    lines = f.readlines()

# The branch fix incorrectly left function bodies at 16-space indent.
# Functions are at 4-space indent (mod is at 0), so bodies must be at 8 spaces.

# Fix range: from after "fn fresh_small_set_date_window_is_scoring_not_filter() {" (17870)
# to line 17905 (closing brace of price_extraction_broadened)

fixed = 0
for i in range(17870, 17905):  # 0-indexed: 17870-17904
    line = lines[i]
    if line.strip() == '':
        continue
    # Count leading spaces
    stripped = line.lstrip(' ')
    n_spaces = len(line) - len(stripped)
    
    if n_spaces >= 16:
        # Dedent: 16→8 for function body, 20→12 for inner content
        if n_spaces == 16:
            lines[i] = ' ' * 8 + stripped
            fixed += 1
        elif n_spaces == 20:
            lines[i] = ' ' * 12 + stripped
            fixed += 1
        elif n_spaces == 12:
            # closing brace should be 4
            lines[i] = ' ' * 4 + stripped
            fixed += 1
        else:
            print(f"  Unexpected indent {n_spaces} at line {i+1}: {repr(line[:40])}")

print(f"Fixed {fixed} lines")

# Verify
for i in range(17865, 17910):
    stripped = lines[i].lstrip(' ')
    n_spaces = len(lines[i]) - len(stripped)
    if stripped:
        expected = 4 if stripped.startswith('}') or stripped.startswith('#[test]') or stripped.startswith('fn ') else 8
        if stripped.startswith('assert') or stripped.startswith('let '):
            expected = 8
        if n_spaces != expected and not stripped.startswith('//') and not stripped.startswith('let nodate') and not stripped.startswith('assert!(!nodate') and not stripped.startswith('assert!(!fresh') and not stripped.startswith('assert!(old'):
            if n_spaces == 16 or n_spaces == 20:
                print(f"  STILL WRONG at line {i+1}: {n_spaces} spaces (expected ~{expected}): {repr(lines[i][:50])}")

with open(path, 'w') as f:
    f.writelines(lines)
print("File written.")
