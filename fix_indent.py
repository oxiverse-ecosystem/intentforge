with open('services/gateway/src/main.rs', 'r') as f:
    lines = f.readlines()

print(f"Total lines: {len(lines)}")

# Check current state of problematic lines
for ln in [17891, 17896, 17922, 17933, 17934]:
    if ln - 1 < len(lines):
        print(f"  {ln}: {repr(lines[ln-1])}")

# Fix: line 17891 (idx 17890) - "            }\n" (12 spaces) -> "    }\n" (4 spaces)
if lines[17890].strip() == '}' and lines[17890].startswith('            '):
    lines[17890] = "    }\n"
    print(f"Fixed 17891")

# Fix: line 17896 (idx 17895) - "            }\n" (12 spaces) -> "    }\n" (4 spaces)
if lines[17895].strip() == '}' and lines[17895].startswith('            '):
    lines[17895] = "    }\n"
    print(f"Fixed 17896")

# Fix: lines 17898-17922 (idx 17897-17921) - dedent 8 spaces (12->4)
for i in range(17897, 17922):
    if lines[i].startswith("            "):  # 12 spaces
        lines[i] = lines[i][8:]

# Fix: lines 17924-17933 (idx 17923-17932) - dedent 12 spaces (16->4)
for i in range(17923, 17933):
    if lines[i].startswith("                "):  # 16 spaces
        lines[i] = lines[i][12:]

# Remove any stray tab-indented brace at line 18651 area
for i in range(18645, 18655):
    if i < len(lines) and '\t' in lines[i] and lines[i].strip() == '}':
        # Check if this is the stray one (not a valid indent)
        print(f"Found stray tab-brace at {i+1}: {repr(lines[i])}")
        # We need to verify this is the closing brace for mod constraint_fix_tests
        # The mod is at 0-indent, so its closing brace should also be at 0-indent
        # Replace with properly indented "}"
        lines[i] = "}\n"
        print(f"Fixed to: {repr(lines[i])}")

# Verify after fixes
print("\n--- After fix ---")
for ln in [17890, 17891, 17895, 17896, 17921, 17922, 17932, 17933, 17934, 17935]:
    if ln - 1 < len(lines):
        print(f"  {ln}: {repr(lines[ln-1])}")

with open('services/gateway/src/main.rs', 'w') as f:
    f.writelines(lines)
print("\nFile written.")
